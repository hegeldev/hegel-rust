use super::*;
use crate::native::core::ChoiceValue;
use alloc::vec;

fn span(start: usize, end: usize, label: u64, depth: u32, parent: Option<usize>) -> Span {
    Span {
        start,
        end,
        label,
        depth,
        parent,
        discarded: parent.is_some(),
    }
}

fn every_kind() -> (Vec<ChoiceNode>, Vec<Span>) {
    let inner = RealizedStream::new(
        vec![ChoiceNode::boolean(BooleanChoice { p: 0.25 }, true, false)],
        vec![span(0, 1, 9, 0, None)],
    );
    let nodes = vec![
        ChoiceNode::integer(
            IntegerChoice {
                min_value: BigInt::from(-300),
                max_value: BigInt::from(1u64 << 40),
                shrink_towards: BigInt::from(5),
            },
            BigInt::from(-7),
            true,
        ),
        ChoiceNode::boolean(BooleanChoice { p: 0.5 }, false, false),
        ChoiceNode::float(
            FloatChoice {
                min_value: -1.5,
                max_value: f64::INFINITY,
                allow_nan: true,
                allow_infinity: false,
                smallest_nonzero_magnitude: 1e-300,
            },
            -0.0,
            false,
        ),
        ChoiceNode::bytes(
            BytesChoice {
                min_size: 1,
                max_size: 8,
            },
            vec![0, 255, 7],
            false,
        ),
        ChoiceNode::string(
            StringChoice {
                intervals: Arc::new(IntervalSet::new(vec![(97, 122), (48, 57)]).unwrap()),
                min_size: 0,
                max_size: 3,
            },
            vec![98, 49],
            false,
        ),
        ChoiceNode::clone_stream(Arc::new(inner), false),
    ];
    let spans = vec![span(0, 6, 1, 0, None), span(2, 4, 2, 1, Some(0))];
    (nodes, spans)
}

#[test]
fn every_kind_round_trips_with_its_constraint_and_the_spans() {
    let (nodes, spans) = every_kind();
    let bytes = serialize_realized(&nodes, &spans).unwrap();
    let (back_nodes, back_spans) = deserialize_realized(&bytes).unwrap();
    assert_eq!(back_nodes, nodes);
    assert_eq!(back_spans, spans);
    assert!(back_nodes[0].was_forced);
    assert_eq!(back_nodes[2].value(), ChoiceValue::Float(-0.0));
    assert!(
        back_nodes[2].value() != ChoiceValue::Float(0.0) || (-0.0f64).to_bits() == 0.0f64.to_bits()
    );
    match &back_nodes[5].data {
        ChoiceData::Clone(stream) => {
            assert_eq!(stream.nodes().len(), 1);
            assert_eq!(stream.spans()[0].label, 9);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn an_empty_case_round_trips() {
    let bytes = serialize_realized(&[], &[]).unwrap();
    assert_eq!(deserialize_realized(&bytes), Some((Vec::new(), Vec::new())));
}

#[test]
fn every_truncation_and_trailing_byte_is_rejected() {
    let (nodes, spans) = every_kind();
    let bytes = serialize_realized(&nodes, &spans).unwrap();
    for cut in 0..bytes.len() {
        assert!(
            deserialize_realized(&bytes[..cut]).is_none(),
            "cut at {cut}"
        );
    }
    let mut longer = bytes.clone();
    longer.push(0);
    assert!(deserialize_realized(&longer).is_none());
}

#[test]
fn another_version_a_bad_tag_a_bad_flag_and_bad_intervals_are_rejected() {
    let (nodes, spans) = every_kind();
    let bytes = serialize_realized(&nodes, &spans).unwrap();
    let mut other_version = bytes.clone();
    other_version[0] = VERSION + 1;
    assert!(deserialize_realized(&other_version).is_none());

    let mut bad_tag = bytes.clone();
    bad_tag[6] = 6;
    assert!(deserialize_realized(&bad_tag).is_none());

    let mut bad_flag = bytes.clone();
    bad_flag[5] = 2;
    assert!(deserialize_realized(&bad_flag).is_none());

    let backwards = ChoiceNode::string(
        StringChoice {
            intervals: Arc::new(IntervalSet::new(vec![(97, 122)]).unwrap()),
            min_size: 0,
            max_size: 3,
        },
        vec![],
        false,
    );
    let mut bytes = serialize_realized(&[backwards], &[]).unwrap();
    let count_at = 1 + 4 + 1 + 1 + 8 + 8;
    bytes[count_at + 4] = 200;
    bytes[count_at + 8] = 100;
    assert!(deserialize_realized(&bytes).is_none());
}

#[test]
fn clones_nested_too_deep_are_refused_both_ways() {
    let mut node = ChoiceNode::boolean(BooleanChoice { p: 0.5 }, false, false);
    for _ in 0..=MAX_CLONE_DEPTH {
        node =
            ChoiceNode::clone_stream(Arc::new(RealizedStream::new(vec![node], Vec::new())), false);
    }
    assert!(serialize_realized(&[node], &[]).is_none());

    let mut bytes = vec![VERSION];
    for _ in 0..=MAX_CLONE_DEPTH + 1 {
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.push(0);
        bytes.push(5);
    }
    assert!(deserialize_realized(&bytes).is_none());
}
