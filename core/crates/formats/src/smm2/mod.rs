//! Super Mario Maker 2 course data and `.bcd` wrapper.

pub mod clear;
pub mod clear_pipe;
pub mod course;
pub mod crypto;
pub mod keyfind;
pub mod sound_effects;
pub mod world;

pub use course::{
    Area, AreaHeader, COURSE_SIZE, Course, CourseHeader, CourseObject, GroundTile, NO_SOUND_EFFECT,
    SoundEffect,
};
pub use crypto::{BCD_SIZE, BcdFile, KeyTable};
