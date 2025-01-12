use crate::array::polygon::PolygonArray;
use crate::array::{
    LineStringArray, MultiLineStringArray, MultiPointArray, MultiPolygonArray, PointArray,
};
use crate::error::WasmResult;
use crate::impl_geometry_array;
#[cfg(feature = "geodesy")]
use crate::reproject::ReprojectDirection;
use wasm_bindgen::prelude::*;

/// An enum of geometry types
#[wasm_bindgen]
pub enum GeometryType {
    Point = 0,
    LineString = 1,
    Polygon = 3,
    MultiPoint = 4,
    MultiLineString = 5,
    MultiPolygon = 6,
}

/// A GeometryArray that can be any of various underlying geometry types
#[wasm_bindgen]
pub struct GeometryArray(pub(crate) geoarrow::array::GeometryArray<i32>);

impl_geometry_array!(GeometryArray);

#[wasm_bindgen]
impl GeometryArray {
    #[wasm_bindgen(js_name = fromPointArray)]
    pub fn from_point_array(arr: PointArray) -> Self {
        Self(geoarrow::array::GeometryArray::Point(arr.0))
    }

    #[wasm_bindgen(js_name = fromLineStringArray)]
    pub fn from_line_string_array(arr: LineStringArray) -> Self {
        Self(geoarrow::array::GeometryArray::LineString(arr.0))
    }

    #[wasm_bindgen(js_name = fromPolygonArray)]
    pub fn from_polygon_array(arr: PolygonArray) -> Self {
        Self(geoarrow::array::GeometryArray::Polygon(arr.0))
    }

    #[wasm_bindgen(js_name = fromMultiPointArray)]
    pub fn from_multi_point_array(arr: MultiPointArray) -> Self {
        Self(geoarrow::array::GeometryArray::MultiPoint(arr.0))
    }

    #[wasm_bindgen(js_name = fromMultiLineStringArray)]
    pub fn from_multi_line_string_array(arr: MultiLineStringArray) -> Self {
        Self(geoarrow::array::GeometryArray::MultiLineString(arr.0))
    }

    #[wasm_bindgen(js_name = fromMultiPolygonArray)]
    pub fn from_multi_polygon_array(arr: MultiPolygonArray) -> Self {
        Self(geoarrow::array::GeometryArray::MultiPolygon(arr.0))
    }

    #[wasm_bindgen(js_name = geometryType)]
    pub fn geometry_type(&self) -> GeometryType {
        match self.0 {
            geoarrow::array::GeometryArray::Point(_) => GeometryType::Point,
            geoarrow::array::GeometryArray::LineString(_) => GeometryType::LineString,
            geoarrow::array::GeometryArray::Polygon(_) => GeometryType::Polygon,
            geoarrow::array::GeometryArray::MultiPoint(_) => GeometryType::MultiPoint,
            geoarrow::array::GeometryArray::MultiLineString(_) => GeometryType::MultiLineString,
            geoarrow::array::GeometryArray::MultiPolygon(_) => GeometryType::MultiPolygon,
            geoarrow::array::GeometryArray::Rect(_) => unimplemented!(),
        }
    }
}

impl From<&GeometryArray> for geoarrow::array::GeometryArray<i32> {
    fn from(value: &GeometryArray) -> Self {
        value.0.clone()
    }
}

impl From<geoarrow::array::GeometryArray<i32>> for GeometryArray {
    fn from(value: geoarrow::array::GeometryArray<i32>) -> Self {
        Self(value)
    }
}
