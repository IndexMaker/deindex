//!
//! Workaround for logging debug messages in tests as console!() macro crashes
//! with SIGSEGV, or code doesn't link.
//!

#[cfg(feature = "debug")]
pub fn print_msg(msg: &str) {
    println!("{}", msg);
}

#[cfg(not(feature = "debug"))]
#[macro_export]
macro_rules! log_msg {
    ($($t:tt)*) => {};
}

#[cfg(feature = "debug")]
#[macro_export]
macro_rules! log_msg {
    ($fmt:literal $(, $args:expr)*) => {
        $crate::log::print_msg(&format!($fmt $(, $args)*));
    };
}
