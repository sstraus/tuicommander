//! Tokio watch adapter for terminal grid frames.

use crate::grid_gate::FrameOrder;
pub(crate) use tuic_terminal::grid_gate::{GridWatchFrame, watch_dropped_frames};

pub(crate) type GridWatchTx = tokio::sync::watch::Sender<GridWatchFrame>;

/// Create a watch channel seeded with the empty frame.
pub(crate) fn new_grid_watch() -> GridWatchTx {
    tokio::sync::watch::channel(GridWatchFrame::default()).0
}

/// Apply the pure frame-order transition and notify readers only for new bytes.
pub(crate) fn claim_grid_frame(tx: &GridWatchTx, order: u64, frame: Option<Vec<u8>>) -> FrameOrder {
    let publishes = frame.is_some();
    let mut verdict = FrameOrder::Stale;
    tx.send_if_modified(|slot| {
        verdict = tuic_terminal::grid_gate::claim_grid_frame(slot, order, frame);
        publishes && verdict == FrameOrder::Newest
    });
    verdict
}

#[cfg(test)]
pub(crate) fn publish_grid_frame(tx: &GridWatchTx, bytes: Vec<u8>) -> FrameOrder {
    let frame = tuic_terminal::grid_gate::GridFrame::cut(bytes);
    claim_grid_frame(tx, frame.order, Some(frame.bytes))
}

/// Release the retained bytes without rewinding the frame sequence.
pub(crate) fn release_grid_frame(tx: &GridWatchTx) {
    tx.send_modify(tuic_terminal::grid_gate::release_grid_frame);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn publish_grid_frame(tx: &GridWatchTx, bytes: Vec<u8>) -> FrameOrder {
        let frame = tuic_terminal::grid_gate::GridFrame::cut(bytes);
        claim_grid_frame(tx, frame.order, Some(frame.bytes))
    }

    #[test]
    fn releasing_the_watch_frame_frees_the_bytes_but_keeps_the_sequence() {
        let tx = new_grid_watch();
        publish_grid_frame(&tx, vec![7u8; 64 * 1024]);
        let seq_before = tx.borrow().seq;

        release_grid_frame(&tx);

        assert_eq!(tx.borrow().frame, [] as [u8; 0]);
        assert_eq!(tx.borrow().seq, seq_before);
    }

    #[test]
    fn a_released_watch_still_publishes_afterwards() {
        let tx = new_grid_watch();
        publish_grid_frame(&tx, vec![1, 2, 3]);
        release_grid_frame(&tx);

        publish_grid_frame(&tx, vec![4, 5]);

        assert_eq!(tx.borrow().frame, vec![4, 5]);
        assert_eq!(tx.borrow().seq, 2);
    }

    #[tokio::test]
    async fn publishing_assigns_consecutive_sequence_numbers() {
        let tx = new_grid_watch();
        let mut rx = tx.subscribe();
        assert_eq!(rx.borrow_and_update().seq, 0);

        publish_grid_frame(&tx, vec![1, 2, 3]);
        rx.changed().await.expect("sender is alive");
        assert_eq!(rx.borrow_and_update().seq, 1);

        publish_grid_frame(&tx, vec![4]);
        rx.changed().await.expect("sender is alive");
        assert_eq!(rx.borrow_and_update().seq, 2);
    }

    #[tokio::test]
    async fn a_slow_reader_can_detect_the_frame_it_never_saw() {
        let tx = new_grid_watch();
        let mut rx = tx.subscribe();
        let last_seq = rx.borrow_and_update().seq;

        publish_grid_frame(&tx, vec![1]);
        publish_grid_frame(&tx, vec![2]);
        rx.changed().await.expect("sender is alive");
        let seq = rx.borrow_and_update().seq;

        assert_eq!(seq, 2);
        assert!(watch_dropped_frames(last_seq, seq));
        assert!(!watch_dropped_frames(seq, seq));
    }
}
