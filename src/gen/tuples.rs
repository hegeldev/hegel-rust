use super::{generate_from_schema, group, labels, Generate};
use serde_json::{json, Value};

pub struct Tuple2Generator<G1, G2> {
    gen1: G1,
    gen2: G2,
}

impl<T1, T2, G1, G2> Generate<(T1, T2)> for Tuple2Generator<G1, G2>
where
    G1: Generate<T1>,
    G2: Generate<T2>,
    T1: serde::de::DeserializeOwned,
    T2: serde::de::DeserializeOwned,
{
    fn generate(&self) -> (T1, T2) {
        if let Some(schema) = self.schema() {
            generate_from_schema(&schema)
        } else {
            group(labels::TUPLE, || {
                let v1 = self.gen1.generate();
                let v2 = self.gen2.generate();
                (v1, v2)
            })
        }
    }

    fn schema(&self) -> Option<Value> {
        let s1 = self.gen1.schema()?;
        let s2 = self.gen2.schema()?;

        Some(json!({
            "type": "array",
            "prefixItems": [s1, s2],
            "items": false,
            "minItems": 2,
            "maxItems": 2
        }))
    }
}

pub fn tuples<T1, T2, G1: Generate<T1>, G2: Generate<T2>>(
    gen1: G1,
    gen2: G2,
) -> Tuple2Generator<G1, G2> {
    Tuple2Generator { gen1, gen2 }
}

pub struct Tuple3Generator<G1, G2, G3> {
    gen1: G1,
    gen2: G2,
    gen3: G3,
}

impl<T1, T2, T3, G1, G2, G3> Generate<(T1, T2, T3)> for Tuple3Generator<G1, G2, G3>
where
    G1: Generate<T1>,
    G2: Generate<T2>,
    G3: Generate<T3>,
    T1: serde::de::DeserializeOwned,
    T2: serde::de::DeserializeOwned,
    T3: serde::de::DeserializeOwned,
{
    fn generate(&self) -> (T1, T2, T3) {
        if let Some(schema) = self.schema() {
            generate_from_schema(&schema)
        } else {
            group(labels::TUPLE, || {
                let v1 = self.gen1.generate();
                let v2 = self.gen2.generate();
                let v3 = self.gen3.generate();
                (v1, v2, v3)
            })
        }
    }

    fn schema(&self) -> Option<Value> {
        let s1 = self.gen1.schema()?;
        let s2 = self.gen2.schema()?;
        let s3 = self.gen3.schema()?;

        Some(json!({
            "type": "array",
            "prefixItems": [s1, s2, s3],
            "items": false,
            "minItems": 3,
            "maxItems": 3
        }))
    }
}

pub fn tuples3<T1, T2, T3, G1: Generate<T1>, G2: Generate<T2>, G3: Generate<T3>>(
    gen1: G1,
    gen2: G2,
    gen3: G3,
) -> Tuple3Generator<G1, G2, G3> {
    Tuple3Generator { gen1, gen2, gen3 }
}
