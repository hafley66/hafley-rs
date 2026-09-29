use hafley_observe as oh;
use tracing_subscriber::prelude::*;

oh::counting_allocator!();

#[test]
fn nested_spans_own_their_allocations() {
    let subscriber = tracing_subscriber::registry().with(oh::allocation::layers());
    tracing::subscriber::with_default(subscriber, || {
        let parent = tracing::info_span!("parent");
        oh::allocation::attach(&parent);
        let _parent = parent.enter();
        let before_parent = oh::allocation::allocated_bytes(&parent).unwrap();
        let parent_buf = std::hint::black_box(vec![0_u8; 1024]);
        let after_parent = oh::allocation::allocated_bytes(&parent).unwrap();
        assert!(after_parent >= before_parent + 1024);

        let child = tracing::info_span!("child");
        oh::allocation::attach(&child);
        let parent_before_child = oh::allocation::allocated_bytes(&parent).unwrap();
        let before_child = oh::allocation::allocated_bytes(&child).unwrap();
        {
            let _child = child.enter();
            let child_buf = std::hint::black_box(vec![0_u8; 2048]);
            assert!(oh::allocation::allocated_bytes(&child).unwrap() >= before_child + 2048);
            std::hint::black_box(child_buf);
        }
        assert_eq!(
            oh::allocation::allocated_bytes(&parent).unwrap(),
            parent_before_child
        );
        std::hint::black_box(parent_buf);
    });
}
