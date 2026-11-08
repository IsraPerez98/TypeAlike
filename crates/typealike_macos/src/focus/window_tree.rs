//! Reads the tree of a window for an app that builds the text of its fields only after an assistive
//! app reads that tree. Word shows its document as an empty layout area until then.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use objc2_application_services::AXUIElement;
use objc2_core_foundation::Type;

use crate::accessibility::{ROLE_ATTRIBUTE, children, copy_string};

/// The most elements that one walk reads. Word builds the text of a document after a walk of about
/// 120 elements.
const MAX_WALKED_ELEMENTS: usize = 200;
/// The deepest level below the window whose elements a walk reads. Word needs 6 levels.
const MAX_WALK_DEPTH: usize = 8;
/// The longest time of one walk on the main thread, for an app that answers slowly.
const MAX_WALK_TIME: Duration = Duration::from_millis(100);
/// A popover can close when an assistive app reads its tree, as the event popover of Calendar does.
const POPOVER_ROLE: &str = "AXPopover";

/// Reads the elements of `window` breadth first, without the elements in a popover.
pub(super) fn walk_window(window: &AXUIElement) {
    let started = Instant::now();
    let visited = walk_breadth_first(
        window.retain(),
        || started.elapsed() > MAX_WALK_TIME,
        |element| {
            if copy_string(element, ROLE_ATTRIBUTE).as_deref() == Some(POPOVER_ROLE) {
                return Vec::new();
            }
            children(element)
        },
    );
    tracing::debug!(
        visited,
        elapsed_ms = started.elapsed().as_millis(),
        "read the tree of the focused window"
    );
}

/// Visits a tree breadth first from `root`, and returns how many elements it visited. It gets the
/// children of each element above [`MAX_WALK_DEPTH`] from `children`. It stops after
/// [`MAX_WALKED_ELEMENTS`] elements, or when `is_out_of_time` is true.
fn walk_breadth_first<T>(
    root: T,
    is_out_of_time: impl Fn() -> bool,
    mut children: impl FnMut(&T) -> Vec<T>,
) -> usize {
    let mut pending = VecDeque::from([(root, 0)]);
    let mut visited = 0;
    while visited < MAX_WALKED_ELEMENTS && !is_out_of_time() {
        let Some((element, depth)) = pending.pop_front() else {
            break;
        };
        visited += 1;
        if depth < MAX_WALK_DEPTH {
            pending.extend(children(&element).into_iter().map(|child| (child, depth + 1)));
        }
    }
    visited
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walk_reads_the_children_of_each_level_above_the_deepest_level() {
        let mut levels_read = Vec::new();

        let visited = walk_breadth_first(
            0,
            || false,
            |&level| {
                levels_read.push(level);
                vec![level + 1]
            },
        );

        assert_eq!(
            (visited, levels_read),
            (MAX_WALK_DEPTH + 1, (0..MAX_WALK_DEPTH).collect())
        );
    }

    #[test]
    fn walk_reads_the_elements_of_one_level_before_the_next_level() {
        let mut elements_read = Vec::new();

        walk_breadth_first(
            1,
            || false,
            |&element| {
                elements_read.push(element);
                if element < 4 {
                    vec![element * 2, element * 2 + 1]
                } else {
                    Vec::new()
                }
            },
        );

        assert_eq!(elements_read, [1, 2, 3, 4, 5, 6, 7]);
    }

    #[test]
    fn walk_stops_after_the_most_elements() {
        let visited = walk_breadth_first(0, || false, |_| vec![0; 50]);

        assert_eq!(visited, MAX_WALKED_ELEMENTS);
    }

    #[test]
    fn walk_stops_when_out_of_time() {
        let visited = walk_breadth_first(0, || true, |_| vec![0; 50]);

        assert_eq!(visited, 0);
    }
}
