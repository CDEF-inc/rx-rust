//! Declares unstable feature names that only std defines, so that
//! coretests' `#![feature(...)]` for them is not an unknown feature. (A
//! crate can only declare them with `staged_api`, which the std stand-in
//! does not use: it would need a stability attribute on every item.)

#![no_std]
#![feature(staged_api)]
#![stable(feature = "rx_std_features", since = "1.0.0")]

#[unstable(feature = "float_gamma", issue = "none")]
fn _float_gamma() {}
