use super::{group, labels, BasicGenerator, Generate};
use crate::cbor_helpers::{cbor_array, cbor_map};
use ciborium::Value;
use std::any::Any;
use std::sync::OnceLock;

pub struct Tuple2Generator<G1, G2> {
    gen1: G1,
    gen2: G2,
    cached_basic: OnceLock<Box<dyn Any + Send + Sync>>,
}

impl<T1, T2, G1, G2> Generate<(T1, T2)> for Tuple2Generator<G1, G2>
where
    G1: Generate<T1>,
    G2: Generate<T2>,
    T1: serde::de::DeserializeOwned + 'static,
    T2: serde::de::DeserializeOwned + 'static,
{
    fn generate(&self) -> (T1, T2) {
        if let Some(basic) = self.as_basic() {
            basic.generate()
        } else {
            group(labels::TUPLE, || {
                let v1 = self.gen1.generate();
                let v2 = self.gen2.generate();
                (v1, v2)
            })
        }
    }

    fn as_basic(&self) -> Option<BasicGenerator<(T1, T2)>> {
        self.cached_basic
            .get_or_init(|| {
                let result: Option<BasicGenerator<(T1, T2)>> = (|| {
                    let b1 = self.gen1.as_basic()?;
                    let b2 = self.gen2.as_basic()?;

                    let schema = cbor_map! {
                        "type" => "tuple",
                        "elements" => cbor_array![b1.schema.clone(), b2.schema.clone()]
                    };

                    let has_transforms = b1.transform.is_some() || b2.transform.is_some();

                    if has_transforms {
                        let t1 = b1.transform;
                        let t2 = b2.transform;

                        Some(BasicGenerator::with_transform(schema, move |raw| {
                            let arr = match raw {
                                Value::Array(arr) => arr,
                                _ => panic!(
                                    "Expected array from tuple schema, got {:?}",
                                    raw
                                ),
                            };
                            let mut iter = arr.into_iter();
                            let v1_raw = iter.next().expect("tuple missing element 0");
                            let v2_raw = iter.next().expect("tuple missing element 1");

                            let v1 = if let Some(ref t) = t1 {
                                t(v1_raw)
                            } else {
                                let hv = super::value::HegelValue::from(v1_raw.clone());
                                super::value::from_hegel_value(hv).unwrap_or_else(|e| {
                                    panic!(
                                        "hegel: failed to deserialize tuple element 0: {}\nValue: {:?}",
                                        e, v1_raw
                                    );
                                })
                            };

                            let v2 = if let Some(ref t) = t2 {
                                t(v2_raw)
                            } else {
                                let hv = super::value::HegelValue::from(v2_raw.clone());
                                super::value::from_hegel_value(hv).unwrap_or_else(|e| {
                                    panic!(
                                        "hegel: failed to deserialize tuple element 1: {}\nValue: {:?}",
                                        e, v2_raw
                                    );
                                })
                            };

                            (v1, v2)
                        }))
                    } else {
                        Some(BasicGenerator::new(schema))
                    }
                })();
                Box::new(result) as Box<dyn Any + Send + Sync>
            })
            .downcast_ref::<Option<BasicGenerator<(T1, T2)>>>()
            .expect("cached_basic type mismatch")
            .clone()
    }
}

pub fn tuples<T1, T2, G1: Generate<T1>, G2: Generate<T2>>(
    gen1: G1,
    gen2: G2,
) -> Tuple2Generator<G1, G2> {
    Tuple2Generator {
        gen1,
        gen2,
        cached_basic: OnceLock::new(),
    }
}

pub struct Tuple3Generator<G1, G2, G3> {
    gen1: G1,
    gen2: G2,
    gen3: G3,
    cached_basic: OnceLock<Box<dyn Any + Send + Sync>>,
}

impl<T1, T2, T3, G1, G2, G3> Generate<(T1, T2, T3)> for Tuple3Generator<G1, G2, G3>
where
    G1: Generate<T1>,
    G2: Generate<T2>,
    G3: Generate<T3>,
    T1: serde::de::DeserializeOwned + 'static,
    T2: serde::de::DeserializeOwned + 'static,
    T3: serde::de::DeserializeOwned + 'static,
{
    fn generate(&self) -> (T1, T2, T3) {
        if let Some(basic) = self.as_basic() {
            basic.generate()
        } else {
            group(labels::TUPLE, || {
                let v1 = self.gen1.generate();
                let v2 = self.gen2.generate();
                let v3 = self.gen3.generate();
                (v1, v2, v3)
            })
        }
    }

    fn as_basic(&self) -> Option<BasicGenerator<(T1, T2, T3)>> {
        self.cached_basic
            .get_or_init(|| {
                let result: Option<BasicGenerator<(T1, T2, T3)>> = (|| {
                    let b1 = self.gen1.as_basic()?;
                    let b2 = self.gen2.as_basic()?;
                    let b3 = self.gen3.as_basic()?;

                    let schema = cbor_map! {
                        "type" => "tuple",
                        "elements" => cbor_array![b1.schema.clone(), b2.schema.clone(), b3.schema.clone()]
                    };

                    let has_transforms =
                        b1.transform.is_some() || b2.transform.is_some() || b3.transform.is_some();

                    if has_transforms {
                        let t1 = b1.transform;
                        let t2 = b2.transform;
                        let t3 = b3.transform;

                        Some(BasicGenerator::with_transform(schema, move |raw| {
                            let arr = match raw {
                                Value::Array(arr) => arr,
                                _ => panic!(
                                    "Expected array from tuple schema, got {:?}",
                                    raw
                                ),
                            };
                            let mut iter = arr.into_iter();
                            let v1_raw = iter.next().expect("tuple missing element 0");
                            let v2_raw = iter.next().expect("tuple missing element 1");
                            let v3_raw = iter.next().expect("tuple missing element 2");

                            let v1 = if let Some(ref t) = t1 {
                                t(v1_raw)
                            } else {
                                let hv = super::value::HegelValue::from(v1_raw.clone());
                                super::value::from_hegel_value(hv).unwrap_or_else(|e| {
                                    panic!(
                                        "hegel: failed to deserialize tuple element 0: {}\nValue: {:?}",
                                        e, v1_raw
                                    );
                                })
                            };

                            let v2 = if let Some(ref t) = t2 {
                                t(v2_raw)
                            } else {
                                let hv = super::value::HegelValue::from(v2_raw.clone());
                                super::value::from_hegel_value(hv).unwrap_or_else(|e| {
                                    panic!(
                                        "hegel: failed to deserialize tuple element 1: {}\nValue: {:?}",
                                        e, v2_raw
                                    );
                                })
                            };

                            let v3 = if let Some(ref t) = t3 {
                                t(v3_raw)
                            } else {
                                let hv = super::value::HegelValue::from(v3_raw.clone());
                                super::value::from_hegel_value(hv).unwrap_or_else(|e| {
                                    panic!(
                                        "hegel: failed to deserialize tuple element 2: {}\nValue: {:?}",
                                        e, v3_raw
                                    );
                                })
                            };

                            (v1, v2, v3)
                        }))
                    } else {
                        Some(BasicGenerator::new(schema))
                    }
                })();
                Box::new(result) as Box<dyn Any + Send + Sync>
            })
            .downcast_ref::<Option<BasicGenerator<(T1, T2, T3)>>>()
            .expect("cached_basic type mismatch")
            .clone()
    }
}

pub fn tuples3<T1, T2, T3, G1: Generate<T1>, G2: Generate<T2>, G3: Generate<T3>>(
    gen1: G1,
    gen2: G2,
    gen3: G3,
) -> Tuple3Generator<G1, G2, G3> {
    Tuple3Generator {
        gen1,
        gen2,
        gen3,
        cached_basic: OnceLock::new(),
    }
}
