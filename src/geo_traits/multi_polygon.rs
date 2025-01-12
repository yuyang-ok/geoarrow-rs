use super::polygon::PolygonTrait;
use geo::{CoordNum, MultiPolygon, Polygon};
use std::iter::Cloned;
use std::slice::Iter;

pub trait MultiPolygonTrait<'a> {
    type T: CoordNum;
    type ItemType: 'a + PolygonTrait<'a, T = Self::T>;
    type Iter: ExactSizeIterator<Item = Self::ItemType>;

    /// An iterator over the Polygons in this MultiPolygon
    fn polygons(&'a self) -> Self::Iter;

    /// The number of polygons in this MultiPolygon
    fn num_polygons(&self) -> usize;

    /// Access to a specified polygon in this MultiPolygon
    /// Will return None if the provided index is out of bounds
    fn polygon(&self, i: usize) -> Option<Self::ItemType>;
}

impl<'a, T: CoordNum + 'a> MultiPolygonTrait<'a> for MultiPolygon<T> {
    type T = T;
    type ItemType = Polygon<Self::T>;
    type Iter = Cloned<Iter<'a, Self::ItemType>>;

    fn polygons(&'a self) -> Self::Iter {
        self.0.iter().cloned()
    }

    fn num_polygons(&self) -> usize {
        self.0.len()
    }

    fn polygon(&self, i: usize) -> Option<Self::ItemType> {
        self.0.get(i).cloned()
    }
}

impl<'a, T: CoordNum + 'a> MultiPolygonTrait<'a> for &MultiPolygon<T> {
    type T = T;
    type ItemType = Polygon<Self::T>;
    type Iter = Cloned<Iter<'a, Self::ItemType>>;

    fn polygons(&'a self) -> Self::Iter {
        self.0.iter().cloned()
    }

    fn num_polygons(&self) -> usize {
        self.0.len()
    }

    fn polygon(&self, i: usize) -> Option<Self::ItemType> {
        self.0.get(i).cloned()
    }
}
