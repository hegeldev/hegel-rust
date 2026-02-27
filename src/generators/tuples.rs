use super::{labels, BasicGenerator, Generate, TestCaseData};
use crate::cbor_helpers::{cbor_array, cbor_map};
use ciborium::Value;
use std::marker::PhantomData;

pub struct Tuple2Generator<G1, G2, T1, T2> {
    gen1: G1,
    gen2: G2,
    _phantom: PhantomData<fn(T1, T2)>,
}

impl<T1, T2, G1, G2> Generate<(T1, T2)> for Tuple2Generator<G1, G2, T1, T2>
where
    G1: Generate<T1>,
    G2: Generate<T2>,
{
    fn do_draw(&self, data: &TestCaseData) -> (T1, T2) {
        if let Some(basic) = self.as_basic() {
            basic.do_draw(data)
        } else {
            data.span_group(labels::TUPLE, || {
                let v1 = self.gen1.do_draw(data);
                let v2 = self.gen2.do_draw(data);
                (v1, v2)
            })
        }
    }

    fn as_basic(&self) -> Option<BasicGenerator<'_, (T1, T2)>> {
        let basic1 = self.gen1.as_basic()?;
        let basic2 = self.gen2.as_basic()?;

        let schema = cbor_map! {
            "type" => "tuple",
            "elements" => cbor_array![basic1.schema().clone(), basic2.schema().clone()]
        };

        Some(BasicGenerator::new(schema, move |raw| {
            let arr = match raw {
                Value::Array(arr) => arr,
                _ => panic!("Expected array from tuple schema, got {:?}", raw),
            };
            let mut iter = arr.into_iter();

            let v1 = basic1.parse_raw(iter.next().expect("tuple missing element 0"));
            let v2 = basic2.parse_raw(iter.next().expect("tuple missing element 1"));

            (v1, v2)
        }))
    }
}

pub fn tuples<T1, T2, G1: Generate<T1>, G2: Generate<T2>>(
    gen1: G1,
    gen2: G2,
) -> Tuple2Generator<G1, G2, T1, T2> {
    Tuple2Generator {
        gen1,
        gen2,
        _phantom: PhantomData,
    }
}

pub struct Tuple3Generator<G1, G2, G3, T1, T2, T3> {
    gen1: G1,
    gen2: G2,
    gen3: G3,
    _phantom: PhantomData<fn(T1, T2, T3)>,
}

impl<T1, T2, T3, G1, G2, G3> Generate<(T1, T2, T3)> for Tuple3Generator<G1, G2, G3, T1, T2, T3>
where
    G1: Generate<T1>,
    G2: Generate<T2>,
    G3: Generate<T3>,
{
    fn do_draw(&self, data: &TestCaseData) -> (T1, T2, T3) {
        if let Some(basic) = self.as_basic() {
            basic.do_draw(data)
        } else {
            data.span_group(labels::TUPLE, || {
                let v1 = self.gen1.do_draw(data);
                let v2 = self.gen2.do_draw(data);
                let v3 = self.gen3.do_draw(data);
                (v1, v2, v3)
            })
        }
    }

    fn as_basic(&self) -> Option<BasicGenerator<'_, (T1, T2, T3)>> {
        let basic1 = self.gen1.as_basic()?;
        let basic2 = self.gen2.as_basic()?;
        let basic3 = self.gen3.as_basic()?;

        let schema = cbor_map! {
            "type" => "tuple",
            "elements" => cbor_array![
                basic1.schema().clone(),
                basic2.schema().clone(),
                basic3.schema().clone()
            ]
        };

        Some(BasicGenerator::new(schema, move |raw| {
            let arr = match raw {
                Value::Array(arr) => arr,
                _ => panic!("Expected array from tuple schema, got {:?}", raw),
            };
            let mut iter = arr.into_iter();

            let v1 = basic1.parse_raw(iter.next().expect("tuple missing element 0"));
            let v2 = basic2.parse_raw(iter.next().expect("tuple missing element 1"));
            let v3 = basic3.parse_raw(iter.next().expect("tuple missing element 2"));

            (v1, v2, v3)
        }))
    }
}

pub fn tuples3<T1, T2, T3, G1: Generate<T1>, G2: Generate<T2>, G3: Generate<T3>>(
    gen1: G1,
    gen2: G2,
    gen3: G3,
) -> Tuple3Generator<G1, G2, G3, T1, T2, T3> {
    Tuple3Generator {
        gen1,
        gen2,
        gen3,
        _phantom: PhantomData,
    }
}
