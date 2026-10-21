//! Course World's ID search (the hub's "ID" button, `L_IDSearchBtn_00`): type a course ID or a
//! maker ID, and the course or the maker's profile opens. The game asks for the ID with the
//! console's keyboard; we type it into `Nwk_CommentPosting_00`'s text form (the comment
//! window), titled with our own words [departure]. The ID is checked with the game's own
//! checksum before anything is asked; a course ID asks the source for the course (`info`), a
//! maker ID opens the maker's profile.

use crate::coursepage::POSTING;
use crate::layout::{Layout, Ui};
use crate::menu::{Buttons, MenuCursor, UiInput, hit};
use crate::online::edit;
use crate::sound::Event;
use crate::{DrawItem, push_layout};

/// "XXX-XXX-XXX" typed with or without the dashes.
const MAX_ID: usize = 11;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SearchEvent {
    Cancel,
    /// A course ID ("XXX-XXX-XXX").
    Course(String),
    /// A maker ID ("XXX-XXX-XXX").
    Maker(String),
}

/// A typed ID → (the ID with dashes, is a maker ID); None: not an SMM2 ID.
pub fn parse_id(text: &str) -> Option<(String, bool)> {
    let raw: String = text
        .chars()
        .filter(|c| !matches!(c, '-' | ' '))
        .map(|c| c.to_ascii_uppercase())
        .collect();
    let (id, maker) = mmk_formats::course_id::smm2_code_to_data_id(&raw).ok()?;
    Some((
        mmk_formats::course_id::smm2_data_id_to_code(id, maker),
        maker,
    ))
}

pub struct IdSearchScene {
    ui: Ui,
    pub layout: Layout,
    cursor: MenuCursor,
    pub tick: i32,
    pub text: String,
    leaving: bool,
    /// Shown under the field after a refused ID.
    note: Option<String>,
}

impl IdSearchScene {
    pub fn new(ui: &Ui) -> Option<Self> {
        let mut l = ui.layout(POSTING)?;
        l.play("In", 0);
        l.hold("Type", 1.0);
        l.text("T_Title_00", "Course ID or Maker ID");
        l.text("L_PostBtn_00/T_Btn_00", "Search");
        for p in [
            "L_TextBtn_00",
            "L_DrawingBtn_00",
            "L_StampBtn_00",
            "L_SpoilerBtn_00",
            "P_DummyThumbnail_00",
            "N_Comment_00",
        ] {
            l.show(p, false);
        }
        let mut s = Self {
            ui: ui.clone(),
            layout: l,
            cursor: MenuCursor::new(ui),
            tick: 0,
            text: String::new(),
            leaving: false,
            note: None,
        };
        s.show_text();
        Some(s)
    }

    pub fn typing(&self) -> bool {
        !self.leaving
    }

    fn show_text(&mut self) {
        let caret = if self.tick / 30 % 2 == 0 { "_" } else { "" };
        let mut t = format!("{}{caret}", self.text);
        if let Some(n) = &self.note {
            t = format!("{t}\n\n{n}");
        }
        self.layout.wrapped_text("T_TextComment_00", &t, 300.0, 4);
    }

    pub fn step(&mut self, input: &UiInput) -> Option<SearchEvent> {
        self.tick += 1;
        if self.leaving {
            return None;
        }
        let mut send = input.pressed.has(Buttons::CONFIRM);
        let mut cancel = input.pressed.has(Buttons::BACK);
        if let Some(p) = input.click {
            let post = ["L_PostBtn_00/B_Btn_00".to_string()];
            send |= hit(&mut self.layout, self.tick, &post, p).is_some();
        }
        match input.text {
            Some('\u{1b}') => cancel = true,
            Some(c) => {
                send |= edit(&mut self.text, c, MAX_ID);
                self.note = None;
            }
            None => {}
        }
        if cancel {
            self.ui.sound_back(POSTING, "normal");
            self.layout.play("Out", self.tick);
            self.leaving = true;
            return Some(SearchEvent::Cancel);
        }
        let mut ev = None;
        if send && !self.text.trim().is_empty() {
            match parse_id(&self.text) {
                Some((id, maker)) => {
                    self.ui.sound_button(POSTING, "L_PostBtn_00", Event::Decide);
                    self.layout.play("L_PostBtn_00/Decide", self.tick);
                    self.layout.play("Out", self.tick);
                    self.leaving = true;
                    ev = Some(if maker {
                        SearchEvent::Maker(id)
                    } else {
                        SearchEvent::Course(id)
                    });
                }
                None => self.note = Some("That isn't a course ID or a maker ID.".into()),
            }
        }
        self.show_text();
        self.cursor
            .place(&mut self.layout, "L_PostBtn_00/B_Btn_00", self.tick, 1.0);
        ev
    }

    pub fn draw(&mut self, out: &mut Vec<DrawItem>) {
        push_layout(out, &mut self.layout, self.tick);
        self.cursor.draw(self.tick, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_parse_with_the_games_checksum() {
        let c = mmk_formats::course_id::smm2_data_id_to_code(5000, false);
        let m = mmk_formats::course_id::smm2_data_id_to_code(77, true);
        assert_eq!(
            parse_id(&c.replace('-', "").to_lowercase()),
            Some((c.clone(), false))
        );
        assert_eq!(parse_id(&format!(" {m} ")), Some((m, true)));
        assert_eq!(parse_id("ABC-DEF-GHJ"), None);
        assert_eq!(parse_id("hello"), None);
    }
}
