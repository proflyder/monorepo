use std::cell::RefCell;

use home_runtime::Supervisor;

thread_local! {
    static SUPERVISOR: RefCell<Option<Supervisor>> = const { RefCell::new(None) };
}

#[must_use]
pub fn supervisor() -> Supervisor {
    SUPERVISOR.with_borrow_mut(|slot| slot.get_or_insert_with(Supervisor::new).clone())
}
