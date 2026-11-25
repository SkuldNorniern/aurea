//! One system sheet at a time: pickers, alerts and share sheets ask here, and one asked for while
//! another shows waits its turn, as a phone only shows one anyway.

#![cfg_attr(
    not(any(test, target_os = "ios", target_os = "android")),
    allow(dead_code, unused_imports)
)]

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;

use crate::AureaResult;

type Waiting = Box<dyn FnOnce()>;

thread_local! {
    static PRESENTED: Cell<bool> = const { Cell::new(false) };
    static QUEUE: RefCell<VecDeque<Waiting>> = const { RefCell::new(VecDeque::new()) };
}

/// Whether a system sheet is up, or about to be. False on the desktop, where dialogs block.
pub fn sheet_presented() -> bool {
    PRESENTED.get()
}

fn next() {
    match QUEUE.with_borrow_mut(VecDeque::pop_front) {
        Some(start) => start(),
        None => PRESENTED.set(false),
    }
}

type Done<T> = Box<dyn FnOnce(T)>;

/// A sheet's turn: `done` takes its answer, or `fallback` when it is queued and ends unanswered.
struct Turn<T> {
    done: Cell<Option<Done<T>>>,
    fallback: Cell<Option<T>>,
    ended: Cell<bool>,
}

/// Ends a turn once, with `answer` or else the fallback, and lets the next sheet up.
fn end<T>(turn: &Turn<T>, answer: Option<T>) {
    if turn.ended.replace(true) {
        return;
    }
    let value = answer.or_else(|| turn.fallback.take());
    if let (Some(done), Some(value)) = (turn.done.take(), value) {
        done(value);
    }
    next();
}

/// The sheet's hold on its turn: dropped unanswered, the turn still ends, so a sheet that goes
/// away cannot keep the ones behind it waiting.
struct Hold<T: 'static>(Rc<Turn<T>>);

impl<T: 'static> Drop for Hold<T> {
    fn drop(&mut self) {
        end(&self.0, None);
    }
}

/// Runs `show` now if no sheet is up, or once those ahead of it are through. `show` is handed what
/// to call with the answer, which runs `done` and lets the next sheet up.
///
/// A sheet that cannot show right now returns its error, and `done` isn't called. One that cannot
/// show when its turn comes has no one to return to, so `done` hears `fallback`. A sheet that is
/// shown and dropped without an answer ends its turn without calling `done`.
pub(crate) fn in_turn<T: 'static>(
    fallback: T,
    done: impl FnOnce(T) + 'static,
    show: impl FnOnce(Box<dyn FnOnce(T)>) -> AureaResult<()> + 'static,
) -> AureaResult<()> {
    let turn = Rc::new(Turn {
        done: Cell::new(Some(Box::new(done) as Box<dyn FnOnce(T)>)),
        fallback: Cell::new(None),
        ended: Cell::new(false),
    });
    let hold = Hold(Rc::clone(&turn));
    let answer = Box::new(move |value: T| end(&hold.0, Some(value))) as Box<dyn FnOnce(T)>;
    if !PRESENTED.get() {
        PRESENTED.set(true);
        let shown = show(answer);
        if shown.is_err() {
            end(&turn, None);
        }
        return shown;
    }
    turn.fallback.set(Some(fallback));
    QUEUE.with_borrow_mut(|queue| {
        queue.push_back(Box::new(move || {
            if let Err(error) = show(answer) {
                log::warn!("sheet: {error}");
                end(&turn, None);
            }
        }));
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AureaError;

    type Answers = Rc<RefCell<Vec<Box<dyn FnOnce(i32)>>>>;
    type Heard = Rc<RefCell<Vec<i32>>>;

    /// A sheet that stays up until its answer is taken from `up`.
    fn ask(up: &Answers, heard: &Heard, name: i32) -> AureaResult<()> {
        let (up, heard) = (Rc::clone(up), Rc::clone(heard));
        in_turn(
            -1,
            move |answer| heard.borrow_mut().push(name * 10 + answer),
            move |answer| {
                up.borrow_mut().push(answer);
                Ok(())
            },
        )
    }

    fn answer(up: &Answers, with: i32) {
        let first = up.borrow_mut().remove(0);
        first(with);
    }

    #[test]
    fn sheets_wait_their_turn() {
        let (up, heard) = (Answers::default(), Heard::default());
        ask(&up, &heard, 1).expect("first");
        ask(&up, &heard, 2).expect("second");
        assert!(sheet_presented());
        assert_eq!(up.borrow().len(), 1, "the second waits");
        answer(&up, 5);
        assert_eq!(*heard.borrow(), [15]);
        assert_eq!(up.borrow().len(), 1, "the second is up now");
        assert!(sheet_presented());
        answer(&up, 6);
        assert_eq!(*heard.borrow(), [15, 26]);
        assert!(!sheet_presented());
    }

    #[test]
    fn asking_from_an_answer_goes_after_those_already_waiting() {
        let (up, heard) = (Answers::default(), Heard::default());
        ask(&up, &heard, 1).expect("first");
        ask(&up, &heard, 2).expect("second");
        let (again_up, again_heard) = (Rc::clone(&up), Rc::clone(&heard));
        let first = up.borrow_mut().remove(0);
        heard.borrow_mut().clear();
        // what the app's `done` would do: ask for another while the next is about to show.
        ask(&again_up, &again_heard, 3).expect("third");
        first(0);
        answer(&up, 1);
        answer(&up, 2);
        assert_eq!(*heard.borrow(), [10, 21, 32]);
        assert!(!sheet_presented());
    }

    #[test]
    fn a_sheet_dropped_unanswered_lets_the_next_up() {
        let (up, heard) = (Answers::default(), Heard::default());
        in_turn(
            -1,
            |_| {},
            |answer| {
                drop(answer);
                Ok(())
            },
        )
        .expect("first");
        ask(&up, &heard, 2).expect("second");
        assert_eq!(
            up.borrow().len(),
            1,
            "the second is up, not queued behind the first"
        );
        assert!(heard.borrow().is_empty(), "no answer, no done");
        answer(&up, 3);
        assert_eq!(*heard.borrow(), [23]);
        assert!(!sheet_presented());
    }

    #[test]
    fn a_sheet_that_cannot_show_says_so_or_answers_for_itself() {
        let (up, heard) = (Answers::default(), Heard::default());
        let failing = |heard: &Heard| {
            let heard = Rc::clone(heard);
            in_turn(
                -1,
                move |answer| heard.borrow_mut().push(answer),
                |_| Err(AureaError::ElementOperationFailed),
            )
        };
        assert!(failing(&heard).is_err(), "right now: an error");
        assert!(heard.borrow().is_empty(), "and no answer");
        assert!(!sheet_presented());

        ask(&up, &heard, 1).expect("first");
        failing(&heard).expect("its turn has not come");
        ask(&up, &heard, 2).expect("third");
        answer(&up, 0);
        assert_eq!(*heard.borrow(), [10, -1], "in its turn: the fallback");
        assert_eq!(up.borrow().len(), 1, "and the next goes on");
        answer(&up, 0);
        assert!(!sheet_presented());
    }
}
