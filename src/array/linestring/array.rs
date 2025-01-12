use std::collections::HashMap;
use std::sync::Arc;

use crate::algorithm::native::eq::offset_buffer_eq;
use crate::array::util::{offsets_buffer_i32_to_i64, offsets_buffer_i64_to_i32, OffsetBufferUtils};
use crate::array::zip_validity::ZipValidity;
use crate::array::{CoordBuffer, CoordType, MultiPointArray, WKBArray};
use crate::datatypes::GeoDataType;
use crate::error::{GeoArrowError, Result};
use crate::scalar::LineString;
use crate::trait_::GeoArrayAccessor;
use crate::util::{owned_slice_offsets, owned_slice_validity};
use crate::GeometryArrayTrait;
use arrow_array::{Array, ArrayRef, GenericListArray, LargeListArray, ListArray, OffsetSizeTrait};
use arrow_buffer::bit_iterator::BitIterator;
use arrow_buffer::{NullBuffer, OffsetBuffer};
use arrow_schema::{DataType, Field, FieldRef};

use super::MutableLineStringArray;

/// An immutable array of LineString geometries using GeoArrow's in-memory representation.
///
/// This is semantically equivalent to `Vec<Option<LineString>>` due to the internal validity
/// bitmap.
#[derive(Debug, Clone)]
pub struct LineStringArray<O: OffsetSizeTrait> {
    // Always GeoDataType::LineString or GeoDataType::LargeLineString
    data_type: GeoDataType,

    pub coords: CoordBuffer,

    /// Offsets into the coordinate array where each geometry starts
    pub geom_offsets: OffsetBuffer<O>,

    /// Validity bitmap
    pub validity: Option<NullBuffer>,
}

pub(super) fn check<O: OffsetSizeTrait>(
    coords: &CoordBuffer,
    validity_len: Option<usize>,
    geom_offsets: &OffsetBuffer<O>,
) -> Result<()> {
    if validity_len.map_or(false, |len| len != geom_offsets.len_proxy()) {
        return Err(GeoArrowError::General(
            "validity mask length must match the number of values".to_string(),
        ));
    }

    if geom_offsets.last().to_usize().unwrap() != coords.len() {
        return Err(GeoArrowError::General(
            "largest geometry offset must match coords length".to_string(),
        ));
    }

    Ok(())
}

impl<O: OffsetSizeTrait> LineStringArray<O> {
    /// Create a new LineStringArray from parts
    ///
    /// # Implementation
    ///
    /// This function is `O(1)`.
    ///
    /// # Panics
    ///
    /// - if the validity is not `None` and its length is different from the number of geometries
    /// - if the largest geometry offset does not match the number of coordinates
    pub fn new(
        coords: CoordBuffer,
        geom_offsets: OffsetBuffer<O>,
        validity: Option<NullBuffer>,
    ) -> Self {
        Self::try_new(coords, geom_offsets, validity).unwrap()
    }

    /// Create a new LineStringArray from parts
    ///
    /// # Implementation
    ///
    /// This function is `O(1)`.
    ///
    /// # Errors
    ///
    /// - if the validity buffer does not have the same length as the number of geometries
    /// - if the geometry offsets do not match the number of coordinates
    pub fn try_new(
        coords: CoordBuffer,
        geom_offsets: OffsetBuffer<O>,
        validity: Option<NullBuffer>,
    ) -> Result<Self> {
        check(&coords, validity.as_ref().map(|v| v.len()), &geom_offsets)?;

        let coord_type = coords.coord_type();
        let data_type = match O::IS_LARGE {
            true => GeoDataType::LargeLineString(coord_type),
            false => GeoDataType::LineString(coord_type),
        };

        Ok(Self {
            data_type,
            coords,
            geom_offsets,
            validity,
        })
    }

    fn vertices_field(&self) -> Arc<Field> {
        Field::new("vertices", self.coords.storage_type(), false).into()
    }

    fn outer_type(&self) -> DataType {
        match O::IS_LARGE {
            true => DataType::LargeList(self.vertices_field()),
            false => DataType::List(self.vertices_field()),
        }
    }
}

impl<'a, O: OffsetSizeTrait> GeometryArrayTrait<'a> for LineStringArray<O> {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn data_type(&self) -> &GeoDataType {
        &self.data_type
    }

    fn storage_type(&self) -> DataType {
        self.outer_type()
    }

    fn extension_field(&self) -> FieldRef {
        let mut field_metadata = HashMap::new();
        field_metadata.insert(
            "ARROW:extension:name".to_string(),
            self.extension_name().to_string(),
        );
        Arc::new(Field::new("", self.storage_type(), true).with_metadata(field_metadata))
    }

    fn extension_name(&self) -> &str {
        "geoarrow.linestring"
    }

    fn into_array_ref(self) -> ArrayRef {
        let vertices_field = self.vertices_field();
        let validity = self.validity;
        let coord_array = self.coords.into_array_ref();
        Arc::new(GenericListArray::new(
            vertices_field,
            self.geom_offsets,
            coord_array,
            validity,
        ))
    }

    fn with_coords(self, coords: CoordBuffer) -> Self {
        assert_eq!(coords.len(), self.coords.len());
        Self::new(coords, self.geom_offsets, self.validity)
    }

    fn coord_type(&self) -> CoordType {
        self.coords.coord_type()
    }

    fn into_coord_type(self, coord_type: CoordType) -> Self {
        Self::new(
            self.coords.into_coord_type(coord_type),
            self.geom_offsets,
            self.validity,
        )
    }

    /// Returns the number of geometries in this array
    #[inline]
    fn len(&self) -> usize {
        // TODO: double check/make helper for this
        self.geom_offsets.len() - 1
    }

    /// Returns the optional validity.
    #[inline]
    fn validity(&self) -> Option<&NullBuffer> {
        self.validity.as_ref()
    }

    /// Slices this [`LineStringArray`] in place.
    ///
    /// # Implementation
    /// This operation is `O(1)` as it amounts to increase two ref counts.
    /// # Examples
    /// ```
    /// use arrow::array::PrimitiveArray;
    /// use arrow_array::types::Int32Type;
    ///
    /// let array: PrimitiveArray<Int32Type> = PrimitiveArray::from(vec![1, 2, 3]);
    /// assert_eq!(format!("{:?}", array), "PrimitiveArray<Int32>\n[\n  1,\n  2,\n  3,\n]");
    /// let sliced = array.slice(1, 1);
    /// assert_eq!(format!("{:?}", sliced), "PrimitiveArray<Int32>\n[\n  2,\n]");
    /// // note: `sliced` and `array` share the same memory region.
    /// ```
    /// # Panic
    /// This function panics iff `offset + length > self.len()`.
    #[inline]
    fn slice(&self, offset: usize, length: usize) -> Self {
        assert!(
            offset + length <= self.len(),
            "offset + length may not exceed length of array"
        );
        // Note: we **only** slice the geom_offsets and not any actual data. Otherwise the offsets
        // would be in the wrong location.
        Self {
            data_type: self.data_type.clone(),
            coords: self.coords.clone(),
            geom_offsets: self.geom_offsets.slice(offset, length),
            validity: self.validity.as_ref().map(|v| v.slice(offset, length)),
        }
    }

    fn owned_slice(&self, offset: usize, length: usize) -> Self {
        assert!(
            offset + length <= self.len(),
            "offset + length may not exceed length of array"
        );
        assert!(length >= 1, "length must be at least 1");

        // Find the start and end of the coord buffer
        let (start_coord_idx, _) = self.geom_offsets.start_end(offset);
        let (_, end_coord_idx) = self.geom_offsets.start_end(offset + length - 1);

        let geom_offsets = owned_slice_offsets(&self.geom_offsets, offset, length);

        let coords = self
            .coords
            .owned_slice(start_coord_idx, end_coord_idx - start_coord_idx);

        let validity = owned_slice_validity(self.nulls(), offset, length);

        Self::new(coords, geom_offsets, validity)
    }
}

impl<'a, O: OffsetSizeTrait> GeoArrayAccessor<'a> for LineStringArray<O> {
    type Item = LineString<'a, O>;
    type ItemGeo = geo::LineString;

    unsafe fn value_unchecked(&'a self, index: usize) -> Self::Item {
        LineString::new_borrowed(&self.coords, &self.geom_offsets, index)
    }
}

// Implement geometry accessors
impl<O: OffsetSizeTrait> LineStringArray<O> {
    /// Iterator over geo Geometry objects, not looking at validity
    pub fn iter_geo_values(&self) -> impl Iterator<Item = geo::LineString> + '_ {
        (0..self.len()).map(|i| self.value_as_geo(i))
    }

    /// Iterator over geo Geometry objects, taking into account validity
    pub fn iter_geo(
        &self,
    ) -> ZipValidity<geo::LineString, impl Iterator<Item = geo::LineString> + '_, BitIterator> {
        ZipValidity::new_with_validity(self.iter_geo_values(), self.nulls())
    }

    /// Returns the value at slot `i` as a GEOS geometry.
    #[cfg(feature = "geos")]
    pub fn value_as_geos(&self, i: usize) -> geos::Geometry {
        self.value(i).try_into().unwrap()
    }

    /// Gets the value at slot `i` as a GEOS geometry, additionally checking the validity bitmap
    #[cfg(feature = "geos")]
    pub fn get_as_geos(&self, i: usize) -> Option<geos::Geometry> {
        if self.is_null(i) {
            return None;
        }

        Some(self.value_as_geos(i))
    }

    /// Iterator over GEOS geometry objects
    #[cfg(feature = "geos")]
    pub fn iter_geos_values(&self) -> impl Iterator<Item = geos::Geometry> + '_ {
        (0..self.len()).map(|i| self.value_as_geos(i))
    }

    /// Iterator over GEOS geometry objects, taking validity into account
    #[cfg(feature = "geos")]
    pub fn iter_geos(
        &self,
    ) -> ZipValidity<geos::Geometry, impl Iterator<Item = geos::Geometry> + '_, BitIterator> {
        ZipValidity::new_with_validity(self.iter_geos_values(), self.nulls())
    }
}

impl<O: OffsetSizeTrait> TryFrom<&GenericListArray<O>> for LineStringArray<O> {
    type Error = GeoArrowError;

    fn try_from(value: &GenericListArray<O>) -> Result<Self> {
        let coords: CoordBuffer = value.values().as_ref().try_into()?;
        let geom_offsets = value.offsets();
        let validity = value.nulls();

        Ok(Self::new(coords, geom_offsets.clone(), validity.cloned()))
    }
}

impl TryFrom<&dyn Array> for LineStringArray<i32> {
    type Error = GeoArrowError;

    fn try_from(value: &dyn Array) -> Result<Self> {
        match value.data_type() {
            DataType::List(_) => {
                let downcasted = value.as_any().downcast_ref::<ListArray>().unwrap();
                downcasted.try_into()
            }
            DataType::LargeList(_) => {
                let downcasted = value.as_any().downcast_ref::<LargeListArray>().unwrap();
                let geom_array: LineStringArray<i64> = downcasted.try_into()?;
                geom_array.try_into()
            }
            _ => Err(GeoArrowError::General(format!(
                "Unexpected type: {:?}",
                value.data_type()
            ))),
        }
    }
}

impl TryFrom<&dyn Array> for LineStringArray<i64> {
    type Error = GeoArrowError;

    fn try_from(value: &dyn Array) -> Result<Self> {
        match value.data_type() {
            DataType::List(_) => {
                let downcasted = value.as_any().downcast_ref::<ListArray>().unwrap();
                let geom_array: LineStringArray<i32> = downcasted.try_into()?;
                Ok(geom_array.into())
            }
            DataType::LargeList(_) => {
                let downcasted = value.as_any().downcast_ref::<LargeListArray>().unwrap();
                downcasted.try_into()
            }
            _ => Err(GeoArrowError::General(format!(
                "Unexpected type: {:?}",
                value.data_type()
            ))),
        }
    }
}

impl<O: OffsetSizeTrait> From<Vec<Option<geo::LineString>>> for LineStringArray<O> {
    fn from(other: Vec<Option<geo::LineString>>) -> Self {
        let mut_arr: MutableLineStringArray<O> = other.into();
        mut_arr.into()
    }
}

impl<O: OffsetSizeTrait> From<Vec<geo::LineString>> for LineStringArray<O> {
    fn from(other: Vec<geo::LineString>) -> Self {
        let mut_arr: MutableLineStringArray<O> = other.into();
        mut_arr.into()
    }
}

impl<O: OffsetSizeTrait> From<bumpalo::collections::Vec<'_, Option<geo::LineString>>>
    for LineStringArray<O>
{
    fn from(other: bumpalo::collections::Vec<'_, Option<geo::LineString>>) -> Self {
        let mut_arr: MutableLineStringArray<O> = other.into();
        mut_arr.into()
    }
}

impl<O: OffsetSizeTrait> From<bumpalo::collections::Vec<'_, geo::LineString>>
    for LineStringArray<O>
{
    fn from(other: bumpalo::collections::Vec<'_, geo::LineString>) -> Self {
        let mut_arr: MutableLineStringArray<O> = other.into();
        mut_arr.into()
    }
}

/// LineString and MultiPoint have the same layout, so enable conversions between the two to change
/// the semantic type
impl<O: OffsetSizeTrait> From<LineStringArray<O>> for MultiPointArray<O> {
    fn from(value: LineStringArray<O>) -> Self {
        Self::new(value.coords, value.geom_offsets, value.validity)
    }
}

impl<O: OffsetSizeTrait> TryFrom<WKBArray<O>> for LineStringArray<O> {
    type Error = GeoArrowError;

    fn try_from(value: WKBArray<O>) -> Result<Self> {
        let mut_arr: MutableLineStringArray<O> = value.try_into()?;
        Ok(mut_arr.into())
    }
}

impl From<LineStringArray<i32>> for LineStringArray<i64> {
    fn from(value: LineStringArray<i32>) -> Self {
        Self::new(
            value.coords,
            offsets_buffer_i32_to_i64(&value.geom_offsets),
            value.validity,
        )
    }
}

impl TryFrom<LineStringArray<i64>> for LineStringArray<i32> {
    type Error = GeoArrowError;

    fn try_from(value: LineStringArray<i64>) -> Result<Self> {
        Ok(Self::new(
            value.coords,
            offsets_buffer_i64_to_i32(&value.geom_offsets)?,
            value.validity,
        ))
    }
}

/// Default to an empty array
impl<O: OffsetSizeTrait> Default for LineStringArray<O> {
    fn default() -> Self {
        MutableLineStringArray::default().into()
    }
}

impl<O: OffsetSizeTrait> PartialEq for LineStringArray<O> {
    fn eq(&self, other: &Self) -> bool {
        if self.validity != other.validity {
            return false;
        }

        if !offset_buffer_eq(&self.geom_offsets, &other.geom_offsets) {
            return false;
        }

        if self.coords != other.coords {
            return false;
        }

        true
    }
}

#[cfg(test)]
mod test {
    use crate::test::geoarrow_data::{
        example_linestring_interleaved, example_linestring_separated, example_linestring_wkb,
    };
    use crate::test::linestring::{ls0, ls1};

    use super::*;

    #[test]
    fn geo_roundtrip_accurate() {
        let arr: LineStringArray<i64> = vec![ls0(), ls1()].into();
        assert_eq!(arr.value_as_geo(0), ls0());
        assert_eq!(arr.value_as_geo(1), ls1());
    }

    #[test]
    fn geo_roundtrip_accurate_option_vec() {
        let arr: LineStringArray<i64> = vec![Some(ls0()), Some(ls1()), None].into();
        assert_eq!(arr.get_as_geo(0), Some(ls0()));
        assert_eq!(arr.get_as_geo(1), Some(ls1()));
        assert_eq!(arr.get_as_geo(2), None);
    }

    // #[test]
    // fn rstar_integration() {
    //     let arr: LineStringArray = vec![ls0(), ls1()].into();
    //     let tree = arr.rstar_tree();

    //     let search_box = AABB::from_corners([3.5, 5.5], [4.5, 6.5]);
    //     let results: Vec<&crate::scalar::LineString> =
    //         tree.locate_in_envelope_intersecting(&search_box).collect();

    //     assert_eq!(results.len(), 1);
    //     assert_eq!(
    //         results[0].geom_index, 1,
    //         "The second element in the LineStringArray should be found"
    //     );
    // }

    #[test]
    fn slice() {
        let arr: LineStringArray<i64> = vec![ls0(), ls1()].into();
        let sliced = arr.slice(1, 1);
        assert_eq!(sliced.len(), 1);
        assert_eq!(sliced.get_as_geo(0), Some(ls1()));
    }

    #[test]
    fn owned_slice() {
        let arr: LineStringArray<i64> = vec![ls0(), ls1()].into();
        let sliced = arr.owned_slice(1, 1);

        // assert!(
        //     !sliced.geom_offsets.buffer().is_sliced(),
        //     "underlying offsets should not be sliced"
        // );
        assert_eq!(arr.len(), 2);
        assert_eq!(sliced.len(), 1);
        assert_eq!(sliced.get_as_geo(0), Some(ls1()));
    }

    #[test]
    fn parse_wkb_geoarrow_interleaved_example() {
        let linestring_arr = example_linestring_interleaved();

        let wkb_arr = example_linestring_wkb();
        let parsed_linestring_arr: LineStringArray<i64> = wkb_arr.try_into().unwrap();

        assert_eq!(linestring_arr, parsed_linestring_arr);
    }

    #[test]
    fn parse_wkb_geoarrow_separated_example() {
        let linestring_arr = example_linestring_separated().into_coord_type(CoordType::Interleaved);

        let wkb_arr = example_linestring_wkb();
        let parsed_linestring_arr: LineStringArray<i64> = wkb_arr.try_into().unwrap();

        assert_eq!(linestring_arr, parsed_linestring_arr);
    }
}
