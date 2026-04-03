//! Leader key sequences.
//!
//! A [`LeaderRecognizer`] walks a compile-time trie of [`LeaderNode`]s as keys
//! are pressed after the leader key is activated. Each node matches one
//! key position; leaf nodes carry the action to fire.
//!
//! The trie is a `&'static [LeaderNode<'static>]` — define it as a `const` in
//! your board crate and pass a reference to [`LeaderRecognizer::new`].

use crate::action::KeyCode;
use crate::event::{KeyEvent, KeyPos};
use crate::recognizer::{Recognition, Recognizer};

// ── LeaderNode ────────────────────────────────────────────────────────────────

/// A node in the leader-key trie.
///
/// Each node matches one key position. If `action` is `Some`, the node is a
/// leaf and that keycode is emitted on match. `children` allows further key
/// sequences below this node.
#[derive(Debug, Clone, Copy)]
pub struct LeaderNode<'a> {
    /// The key position this node matches.
    pub key: KeyPos,
    /// The action to fire if this is the final key in the sequence.
    pub action: Option<KeyCode>,
    /// Sub-nodes reachable after this key.
    pub children: &'a [LeaderNode<'a>],
}

// ── LeaderRecognizer ──────────────────────────────────────────────────────────

/// Recognizes a leader-key sequence by walking a trie.
///
/// # Lifecycle
///
/// 1. Construct with [`LeaderRecognizer::new`], passing the trie root and a
///    per-step timeout.
/// 2. Feed key-press events via [`observe`](Recognizer::observe).
/// 3. On a matching leaf: [`Recognition::Matched`] with the action keycode.
/// 4. On no match or timeout: [`Recognition::Failed`].
///
/// Only press events are used; releases are ignored.
pub struct LeaderRecognizer<'a> {
    root: &'a [LeaderNode<'a>],
    current: &'a [LeaderNode<'a>],
    timeout_ms: u64,
    last_ms: Option<u64>,
    done: bool,
}

impl<'a> LeaderRecognizer<'a> {
    /// Creates a new leader recognizer.
    ///
    /// `timeout_ms` is the maximum time (in milliseconds) allowed between
    /// consecutive key presses in the sequence.
    #[must_use]
    pub fn new(root: &'a [LeaderNode<'a>], timeout_ms: u64) -> Self {
        Self {
            root,
            current: root,
            timeout_ms,
            last_ms: None,
            done: false,
        }
    }
}

impl<'a> Recognizer for LeaderRecognizer<'a> {
    type Output = KeyCode;

    fn observe(&mut self, event: KeyEvent, now_ms: u64) -> Recognition<KeyCode> {
        if self.done {
            return Recognition::Pending;
        }

        if !event.pressed {
            return Recognition::Pending;
        }

        if let Some(last) = self.last_ms {
            if now_ms.wrapping_sub(last) >= self.timeout_ms {
                self.done = true;
                return Recognition::Failed;
            }
        }

        self.last_ms = Some(now_ms);

        match self.current.iter().find(|n| n.key == event.pos) {
            Some(node) => {
                if node.children.is_empty() || node.action.is_some() {
                    self.done = true;
                    match node.action {
                        Some(kc) => Recognition::Matched(kc),
                        None => Recognition::Failed,
                    }
                } else {
                    self.current = node.children;
                    Recognition::Pending
                }
            }
            None => {
                self.done = true;
                Recognition::Failed
            }
        }
    }

    fn tick(&mut self, now_ms: u64) -> Recognition<KeyCode> {
        if self.done {
            return Recognition::Pending;
        }
        if let Some(last) = self.last_ms {
            if now_ms.wrapping_sub(last) >= self.timeout_ms {
                self.done = true;
                return Recognition::Failed;
            }
        }
        Recognition::Pending
    }

    fn next_deadline_ms(&self) -> Option<u64> {
        if self.done {
            return None;
        }
        self.last_ms.map(|l| l.wrapping_add(self.timeout_ms))
    }

    fn reset(&mut self) {
        self.current = self.root;
        self.last_ms = None;
        self.done = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TIMEOUT: u64 = 1000;

    #[test]
    fn single_key_sequence_matches() {
        static TRIE: &[LeaderNode<'static>] = &[LeaderNode {
            key: KeyPos::new(0, 0),
            action: Some(KeyCode::Enter),
            children: &[],
        }];
        let mut rec = LeaderRecognizer::new(TRIE, TIMEOUT);
        let result = rec.observe(KeyEvent::press(KeyPos::new(0, 0), 0), 0);
        assert_eq!(result, Recognition::Matched(KeyCode::Enter));
    }

    #[test]
    fn fails_on_unrecognised_key() {
        static TRIE: &[LeaderNode<'static>] = &[LeaderNode {
            key: KeyPos::new(0, 1),
            action: Some(KeyCode::Enter),
            children: &[],
        }];
        let mut rec = LeaderRecognizer::new(TRIE, TIMEOUT);
        let result = rec.observe(KeyEvent::press(KeyPos::new(0, 0), 0), 0);
        assert!(result.is_failed());
    }

    #[test]
    fn multi_step_sequence_matches() {
        static CHILD: &[LeaderNode<'static>] = &[LeaderNode {
            key: KeyPos::new(0, 1),
            action: Some(KeyCode::Tab),
            children: &[],
        }];
        static TRIE: &[LeaderNode<'static>] = &[LeaderNode {
            key: KeyPos::new(0, 0),
            action: None,
            children: CHILD,
        }];
        let mut rec = LeaderRecognizer::new(TRIE, TIMEOUT);
        assert!(rec
            .observe(KeyEvent::press(KeyPos::new(0, 0), 0), 0)
            .is_pending());
        let result = rec.observe(KeyEvent::press(KeyPos::new(0, 1), 10), 10);
        assert_eq!(result, Recognition::Matched(KeyCode::Tab));
    }

    #[test]
    fn wrong_second_key_fails() {
        static CHILD: &[LeaderNode<'static>] = &[LeaderNode {
            key: KeyPos::new(0, 1),
            action: Some(KeyCode::Tab),
            children: &[],
        }];
        static TRIE: &[LeaderNode<'static>] = &[LeaderNode {
            key: KeyPos::new(0, 0),
            action: None,
            children: CHILD,
        }];
        let mut rec = LeaderRecognizer::new(TRIE, TIMEOUT);
        assert!(rec
            .observe(KeyEvent::press(KeyPos::new(0, 0), 0), 0)
            .is_pending());
        let result = rec.observe(KeyEvent::press(KeyPos::new(0, 2), 10), 10);
        assert!(result.is_failed());
    }

    #[test]
    fn intermediate_node_without_action_fails_when_leaf() {
        static TRIE: &[LeaderNode<'static>] = &[LeaderNode {
            key: KeyPos::new(0, 0),
            action: None,
            children: &[],
        }];
        let mut rec = LeaderRecognizer::new(TRIE, TIMEOUT);
        let result = rec.observe(KeyEvent::press(KeyPos::new(0, 0), 0), 0);
        assert!(result.is_failed());
    }

    #[test]
    fn release_events_are_ignored() {
        static TRIE: &[LeaderNode<'static>] = &[];
        let mut rec = LeaderRecognizer::new(TRIE, TIMEOUT);
        let result = rec.observe(KeyEvent::release(KeyPos::new(0, 0), 0), 0);
        assert!(result.is_pending());
    }

    #[test]
    fn timeout_via_tick_causes_failure() {
        static TRIE: &[LeaderNode<'static>] = &[];
        let mut rec = LeaderRecognizer::new(TRIE, TIMEOUT);
        rec.last_ms = Some(0);
        let result = rec.tick(TIMEOUT + 1);
        assert!(result.is_failed());
    }

    #[test]
    fn tick_before_timeout_is_pending() {
        static TRIE: &[LeaderNode<'static>] = &[];
        let mut rec = LeaderRecognizer::new(TRIE, TIMEOUT);
        rec.observe(KeyEvent::press(KeyPos::new(0, 0), 0), 0);
        let _ = rec.observe(KeyEvent::press(KeyPos::new(0, 0), 1), 1);
        assert!(rec.tick(TIMEOUT - 1).is_pending());
    }

    #[test]
    fn observe_times_out_between_steps() {
        static TRIE: &[LeaderNode<'static>] = &[];
        let mut rec = LeaderRecognizer::new(TRIE, 100);
        rec.last_ms = Some(0);
        let result = rec.observe(KeyEvent::press(KeyPos::new(0, 0), 200), 200);
        assert!(result.is_failed());
    }

    #[test]
    fn next_deadline_none_with_no_activity() {
        static TRIE: &[LeaderNode<'static>] = &[];
        let rec = LeaderRecognizer::new(TRIE, TIMEOUT);
        assert!(rec.next_deadline_ms().is_none());
    }

    #[test]
    fn next_deadline_set_after_activity() {
        static TRIE: &[LeaderNode<'static>] = &[];
        let mut rec = LeaderRecognizer::new(TRIE, TIMEOUT);
        rec.last_ms = Some(100);
        assert_eq!(rec.next_deadline_ms(), Some(100 + TIMEOUT));
    }

    #[test]
    fn next_deadline_none_after_done() {
        static TRIE: &[LeaderNode<'static>] = &[];
        let mut rec = LeaderRecognizer::new(TRIE, TIMEOUT);
        rec.last_ms = Some(0);
        rec.tick(TIMEOUT + 1);
        assert!(rec.next_deadline_ms().is_none());
    }

    #[test]
    fn observe_after_done_is_pending() {
        static TRIE: &[LeaderNode<'static>] = &[];
        let mut rec = LeaderRecognizer::new(TRIE, TIMEOUT);
        rec.observe(KeyEvent::press(KeyPos::new(0, 0), 0), 0);
        let result = rec.observe(KeyEvent::press(KeyPos::new(0, 1), 10), 10);
        assert!(result.is_pending());
    }

    #[test]
    fn tick_after_done_is_pending() {
        static TRIE: &[LeaderNode<'static>] = &[];
        let mut rec = LeaderRecognizer::new(TRIE, TIMEOUT);
        rec.done = true;
        assert!(rec.tick(9999).is_pending());
    }

    #[test]
    fn tick_with_no_activity_is_pending() {
        static TRIE: &[LeaderNode<'static>] = &[];
        let mut rec = LeaderRecognizer::new(TRIE, TIMEOUT);
        assert!(rec.tick(500).is_pending());
    }

    #[test]
    fn tick_with_recent_activity_is_pending() {
        static TRIE: &[LeaderNode<'static>] = &[LeaderNode {
            key: KeyPos::new(0, 0),
            action: None,
            children: &[],
        }];
        let mut rec = LeaderRecognizer::new(TRIE, TIMEOUT);
        rec.last_ms = Some(0);
        assert!(rec.tick(TIMEOUT - 1).is_pending());
    }

    #[test]
    fn reset_restores_initial_state() {
        static TRIE: &[LeaderNode<'static>] = &[];
        let mut rec = LeaderRecognizer::new(TRIE, TIMEOUT);
        rec.last_ms = Some(100);
        rec.done = true;
        rec.reset();
        assert!(rec.next_deadline_ms().is_none());
        assert!(!rec.done);
    }
}
