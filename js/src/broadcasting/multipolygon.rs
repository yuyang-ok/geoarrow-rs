use crate::array::MultiPolygonArray;
use crate::scalar::MultiPolygon;
use wasm_bindgen::prelude::*;

enum _BroadcastableMultiPolygon {
    Scalar(geoarrow::scalar::OwnedMultiPolygon<i32>),
    Array(geoarrow::array::MultiPolygonArray<i32>),
}

#[wasm_bindgen]
pub struct BroadcastableMultiPolygon(_BroadcastableMultiPolygon);

#[wasm_bindgen]
impl BroadcastableMultiPolygon {
    #[wasm_bindgen(js_name = fromScalar)]
    pub fn from_scalar(value: MultiPolygon) -> Self {
        Self(_BroadcastableMultiPolygon::Scalar(value.into()))
    }

    #[wasm_bindgen(js_name = fromArray)]
    pub fn from_array(values: MultiPolygonArray) -> Self {
        Self(_BroadcastableMultiPolygon::Array(values.0))
    }
}
