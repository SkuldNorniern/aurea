//! The scene both executors draw: draw commands, the display list that
//! holds them in painter order with their bounds and cache keys, and
//! geometry queries against them. No callbacks and no pixels live here.

pub mod command;
pub mod display_list;
pub mod hit_test;
