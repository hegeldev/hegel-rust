use super::{labels, BasicGenerator, DefaultGenerator, Generator, TestCase};
use crate::cbor_utils::{cbor_array, cbor_map};
use ciborium::Value;
use std::marker::PhantomData;

macro_rules! impl_tuple {
    ($name:ident, $fn_name:ident, $(($idx:tt, $field:ident, $G:ident, $T:ident)),+) => {
        pub struct $name<$($G,)+ $($T,)+> {
            $($field: $G,)+
            _phantom: PhantomData<fn($($T,)+)>,
        }

        impl<$($T,)+ $($G,)+> Generator<($($T,)+)> for $name<$($G,)+ $($T,)+>
        where
            $($G: Generator<$T>,)+
        {
            fn do_draw(&self, tc: &TestCase) -> ($($T,)+) {
                if let Some(basic) = self.as_basic() {
                    basic.do_draw(tc)
                } else {
                    tc.start_span(labels::TUPLE);
                    let result = ($(self.$field.do_draw(tc),)+);
                    tc.stop_span(false);
                    result
                }
            }

            fn as_basic(&self) -> Option<BasicGenerator<'_, ($($T,)+)>> {
                $(let $field = self.$field.as_basic()?;)+

                let schema = cbor_map! {
                    "type" => "tuple",
                    "elements" => cbor_array![$($field.schema().clone()),+]
                };

                Some(BasicGenerator::new(schema, move |raw| {
                    let arr = match raw {
                        Value::Array(arr) => arr,
                        _ => panic!("Expected array from tuple schema, got {:?}", raw),
                    };
                    let mut iter = arr.into_iter();

                    ($($field.parse_raw(iter.next().expect(concat!("tuple missing element ", stringify!($idx)))),)+)
                }))
            }
        }

        #[allow(clippy::too_many_arguments)]
        pub fn $fn_name<$($T,)+ $($G: Generator<$T>,)+>(
            $($field: $G,)+
        ) -> $name<$($G,)+ $($T,)+> {
            $name {
                $($field,)+
                _phantom: PhantomData,
            }
        }

        impl<$($T: DefaultGenerator,)+> DefaultGenerator for ($($T,)+)
        where
            $(<$T as DefaultGenerator>::Generator: Send + Sync,)+
        {
            type Generator = $name<$(<$T as DefaultGenerator>::Generator,)+ $($T,)+>;
            fn default_generator() -> Self::Generator {
                $fn_name($(<$T>::default_generator(),)+)
            }
        }
    };
}

impl_tuple!(
    Tuple2Generator,
    tuples2,
    (0, gen1, G1, T1),
    (1, gen2, G2, T2)
);
impl_tuple!(
    Tuple3Generator,
    tuples3,
    (0, gen1, G1, T1),
    (1, gen2, G2, T2),
    (2, gen3, G3, T3)
);
impl_tuple!(
    Tuple4Generator,
    tuples4,
    (0, gen1, G1, T1),
    (1, gen2, G2, T2),
    (2, gen3, G3, T3),
    (3, gen4, G4, T4)
);
impl_tuple!(
    Tuple5Generator,
    tuples5,
    (0, gen1, G1, T1),
    (1, gen2, G2, T2),
    (2, gen3, G3, T3),
    (3, gen4, G4, T4),
    (4, gen5, G5, T5)
);
impl_tuple!(
    Tuple6Generator,
    tuples6,
    (0, gen1, G1, T1),
    (1, gen2, G2, T2),
    (2, gen3, G3, T3),
    (3, gen4, G4, T4),
    (4, gen5, G5, T5),
    (5, gen6, G6, T6)
);
impl_tuple!(
    Tuple7Generator,
    tuples7,
    (0, gen1, G1, T1),
    (1, gen2, G2, T2),
    (2, gen3, G3, T3),
    (3, gen4, G4, T4),
    (4, gen5, G5, T5),
    (5, gen6, G6, T6),
    (6, gen7, G7, T7)
);
impl_tuple!(
    Tuple8Generator,
    tuples8,
    (0, gen1, G1, T1),
    (1, gen2, G2, T2),
    (2, gen3, G3, T3),
    (3, gen4, G4, T4),
    (4, gen5, G5, T5),
    (5, gen6, G6, T6),
    (6, gen7, G7, T7),
    (7, gen8, G8, T8)
);
impl_tuple!(
    Tuple9Generator,
    tuples9,
    (0, gen1, G1, T1),
    (1, gen2, G2, T2),
    (2, gen3, G3, T3),
    (3, gen4, G4, T4),
    (4, gen5, G5, T5),
    (5, gen6, G6, T6),
    (6, gen7, G7, T7),
    (7, gen8, G8, T8),
    (8, gen9, G9, T9)
);
impl_tuple!(
    Tuple10Generator,
    tuples10,
    (0, gen1, G1, T1),
    (1, gen2, G2, T2),
    (2, gen3, G3, T3),
    (3, gen4, G4, T4),
    (4, gen5, G5, T5),
    (5, gen6, G6, T6),
    (6, gen7, G7, T7),
    (7, gen8, G8, T8),
    (8, gen9, G9, T9),
    (9, gen10, G10, T10)
);
impl_tuple!(
    Tuple11Generator,
    tuples11,
    (0, gen1, G1, T1),
    (1, gen2, G2, T2),
    (2, gen3, G3, T3),
    (3, gen4, G4, T4),
    (4, gen5, G5, T5),
    (5, gen6, G6, T6),
    (6, gen7, G7, T7),
    (7, gen8, G8, T8),
    (8, gen9, G9, T9),
    (9, gen10, G10, T10),
    (10, gen11, G11, T11)
);
impl_tuple!(
    Tuple12Generator,
    tuples12,
    (0, gen1, G1, T1),
    (1, gen2, G2, T2),
    (2, gen3, G3, T3),
    (3, gen4, G4, T4),
    (4, gen5, G5, T5),
    (5, gen6, G6, T6),
    (6, gen7, G7, T7),
    (7, gen8, G8, T8),
    (8, gen9, G9, T9),
    (9, gen10, G10, T10),
    (10, gen11, G11, T11),
    (11, gen12, G12, T12)
);
