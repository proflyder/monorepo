//! Build without sendspin-cpp (host `cargo test`, clippy of the station targets): same API,
//! `spawn` always fails, so no handle ever exists.

use std::convert::Infallible;
use std::marker::PhantomData;
use std::rc::Rc;

use tokio::sync::mpsc::UnboundedReceiver;

use crate::{SendspinConfig, SendspinEvent, SendspinInitError, Transport};

#[derive(Clone)]
pub struct SendspinHandle {
    never: Infallible,
    /// `!Send + !Sync` like the real handle.
    _not_send: PhantomData<Rc<()>>,
}

impl SendspinHandle {
    fn never<T>(&self) -> T {
        match self.never {}
    }

    pub fn streaming(&self) -> bool {
        self.never()
    }

    pub fn connected(&self) -> bool {
        self.never()
    }

    pub fn set_duck_gain(&self, _g: f32) {
        let _: () = self.never();
    }

    pub fn report_volume(&self, _volume: f32, _muted: bool) {
        let _: () = self.never();
    }

    pub fn transport(&self, _t: Transport) -> bool {
        self.never()
    }

    pub fn yield_to_local(&self) {
        let _: () = self.never();
    }

    pub fn confirm_pairing_window(&self) {
        let _: () = self.never();
    }

    pub fn goodbye_restart(&self) {
        let _: () = self.never();
    }
}

pub fn spawn(
    _cfg: &SendspinConfig,
    _volume: f32,
    _muted: bool,
    _master_gain: f32,
) -> Result<(SendspinHandle, UnboundedReceiver<SendspinEvent>), SendspinInitError> {
    Err(SendspinInitError::Unsupported)
}
