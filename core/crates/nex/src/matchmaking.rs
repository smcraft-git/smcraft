//! Matchmaking structures in their NEX 4 layout (MatchmakeExtension 0x6D, MatchMaking 0x15,
//! Notifications 0x0E), and the attributes that mark a gathering as SMCraft's.
//!
//! A `MatchmakeSession` is a `Gathering` (base level) plus the session level; with structure
//! headers each level is framed on its own.

use crate::Error;
use crate::stream::{DateTime, Reader, ResultRange, Structure, Variant, Writer};

pub const PROTOCOL_MATCHMAKE_EXTENSION: u16 = 0x6D;
pub const PROTOCOL_MATCHMAKING: u16 = 0x15;
pub const PROTOCOL_MATCHMAKING_EXT: u16 = 0x32;
pub const PROTOCOL_NOTIFICATIONS: u16 = 0x0E;

/// Notification types (category * 1000 + subtype).
pub const NOTIFY_PARTICIPATE: u32 = 3001;
pub const NOTIFY_DISCONNECTED: u32 = 3007;
pub const NOTIFY_END_PARTICIPATION: u32 = 3008;
pub const NOTIFY_GATHERING_UNREGISTERED: u32 = 109_000;
pub const NOTIFY_HOST_CHANGED: u32 = 110_000;

/// SMCraft's sessions: a game mode no Nintendo title uses, and a tag in the first attribute.
/// Nextendo matches gatherings by game mode (and, where a game server asks for it, by the
/// search criteria), so these never meet a console's session.
pub mod smcraft {
    /// "SC".
    pub const GAME_MODE: u32 = 0x5343;
    /// "SMCR", attribute 0.
    pub const TAG: u32 = 0x534D_4352;
    /// The version of SMCraft's own peer protocol, attribute 1: only equal versions meet.
    pub const PROTOCOL: u32 = 1;
    /// Attribute 2: what is played.
    pub const KIND_COOP: u32 = 0;
    pub const KIND_VERSUS: u32 = 1;
    /// The application data starts with this.
    pub const MAGIC: &[u8] = b"SMCRAFT1";
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MatchmakeSession {
    // Gathering
    pub id: u32,
    pub owner: u64,
    pub host: u64,
    pub min_participants: u16,
    pub max_participants: u16,
    pub participation_policy: u32,
    pub policy_argument: u32,
    pub flags: u32,
    pub state: u32,
    pub description: String,
    // MatchmakeSession
    pub game_mode: u32,
    pub attribs: Vec<u32>,
    pub open_participation: bool,
    pub matchmake_system: u32,
    pub application_data: Vec<u8>,
    pub participants: u32,
    pub progress_score: u8,
    pub session_key: Vec<u8>,
    pub option: u32,
    pub params: Vec<(String, Variant)>,
    pub started: DateTime,
    pub user_password: String,
    pub refer_gid: u32,
    pub user_password_enabled: bool,
    pub system_password_enabled: bool,
    pub codeword: String,
}

impl Structure for MatchmakeSession {
    const VERSIONS: &'static [u8] = &[0, 0];
    fn save(&self, level: usize, w: &mut Writer) {
        if level == 0 {
            w.u32(self.id)
                .pid(self.owner)
                .pid(self.host)
                .u16(self.min_participants)
                .u16(self.max_participants)
                .u32(self.participation_policy)
                .u32(self.policy_argument)
                .u32(self.flags)
                .u32(self.state)
                .string(&self.description);
            return;
        }
        w.u32(self.game_mode)
            .list(&self.attribs, |w, a| {
                w.u32(*a);
            })
            .bool(self.open_participation)
            .u32(self.matchmake_system)
            .buffer(&self.application_data)
            .u32(self.participants)
            .u8(self.progress_score)
            .buffer(&self.session_key)
            .u32(self.option)
            .structure(&MatchmakeParam(self.params.clone()))
            .datetime(self.started)
            .string(&self.user_password)
            .u32(self.refer_gid)
            .bool(self.user_password_enabled)
            .bool(self.system_password_enabled)
            .string(&self.codeword);
    }
    fn load(&mut self, level: usize, r: &mut Reader) -> Result<(), Error> {
        if level == 0 {
            self.id = r.u32()?;
            self.owner = r.pid()?;
            self.host = r.pid()?;
            self.min_participants = r.u16()?;
            self.max_participants = r.u16()?;
            self.participation_policy = r.u32()?;
            self.policy_argument = r.u32()?;
            self.flags = r.u32()?;
            self.state = r.u32()?;
            self.description = r.string()?;
            return Ok(());
        }
        self.game_mode = r.u32()?;
        self.attribs = r.list(|r| r.u32())?;
        self.open_participation = r.bool()?;
        self.matchmake_system = r.u32()?;
        self.application_data = r.buffer()?.to_vec();
        self.participants = r.u32()?;
        self.progress_score = r.u8()?;
        self.session_key = r.buffer()?.to_vec();
        self.option = r.u32()?;
        self.params = r.structure::<MatchmakeParam>()?.0;
        self.started = r.datetime()?;
        self.user_password = r.string()?;
        self.refer_gid = r.u32()?;
        self.user_password_enabled = r.bool()?;
        self.system_password_enabled = r.bool()?;
        self.codeword = r.string()?;
        Ok(())
    }
}

impl MatchmakeSession {
    /// An SMCraft session to look for or open: `kind` (co-op / versus), up to `max` players,
    /// `course` naming the course everyone plays (its hash, so a joiner can check it has it).
    pub fn smcraft(kind: u32, max: u16, course: u64) -> Self {
        let mut app = smcraft::MAGIC.to_vec();
        app.extend_from_slice(&course.to_le_bytes());
        Self {
            min_participants: 2,
            max_participants: max,
            participation_policy: 98,
            game_mode: smcraft::GAME_MODE,
            attribs: vec![smcraft::TAG, smcraft::PROTOCOL, kind, course as u32, 0, 0],
            open_participation: true,
            application_data: app,
            ..Default::default()
        }
    }

    /// Whether this is an SMCraft session of the same protocol.
    pub fn is_smcraft(&self) -> bool {
        self.game_mode == smcraft::GAME_MODE
            && self.attribs.first() == Some(&smcraft::TAG)
            && self.attribs.get(1) == Some(&smcraft::PROTOCOL)
            && self.application_data.starts_with(smcraft::MAGIC)
    }

    /// The course hash an SMCraft session was opened with.
    pub fn smcraft_course(&self) -> Option<u64> {
        let rest = self.application_data.strip_prefix(smcraft::MAGIC)?;
        Some(u64::from_le_bytes(rest.get(..8)?.try_into().ok()?))
    }
}

/// The session's parameter map (`@SR`, `@GIR`, …), a structure of its own.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MatchmakeParam(pub Vec<(String, Variant)>);

impl Structure for MatchmakeParam {
    fn save(&self, _: usize, w: &mut Writer) {
        w.map(
            &self.0,
            |w, k| {
                w.string(k);
            },
            |w, v| {
                w.variant(v);
            },
        );
    }
    fn load(&mut self, _: usize, r: &mut Reader) -> Result<(), Error> {
        self.0 = r.map(|r| r.string(), |r| r.variant())?;
        Ok(())
    }
}

/// A search filter: strings that are empty ("any"), a value, or "min,max".
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SearchCriteria {
    pub attribs: Vec<String>,
    pub game_mode: String,
    pub min_participants: String,
    pub max_participants: String,
    pub matchmake_system: String,
    pub vacant_only: bool,
    pub exclude_locked: bool,
    pub exclude_non_host_pid: bool,
    pub selection_method: u32,
    pub vacant_participants: u16,
    pub params: Vec<(String, Variant)>,
    pub exclude_user_password: bool,
    pub exclude_system_password: bool,
    pub refer_gid: u32,
    pub codeword: String,
    pub range: ResultRange,
}

impl SearchCriteria {
    /// Only SMCraft sessions of this protocol, kind and course, with room left.
    pub fn smcraft(kind: u32, course: u64) -> Self {
        Self {
            attribs: vec![
                smcraft::TAG.to_string(),
                smcraft::PROTOCOL.to_string(),
                kind.to_string(),
                (course as u32).to_string(),
            ],
            game_mode: smcraft::GAME_MODE.to_string(),
            vacant_only: true,
            exclude_locked: true,
            vacant_participants: 1,
            exclude_user_password: true,
            exclude_system_password: true,
            ..Default::default()
        }
    }
}

impl Structure for SearchCriteria {
    fn save(&self, _: usize, w: &mut Writer) {
        w.list(&self.attribs, |w, a| {
            w.string(a);
        })
        .string(&self.game_mode)
        .string(&self.min_participants)
        .string(&self.max_participants)
        .string(&self.matchmake_system)
        .bool(self.vacant_only)
        .bool(self.exclude_locked)
        .bool(self.exclude_non_host_pid)
        .u32(self.selection_method)
        .u16(self.vacant_participants)
        .structure(&MatchmakeParam(self.params.clone()))
        .bool(self.exclude_user_password)
        .bool(self.exclude_system_password)
        .u32(self.refer_gid)
        .string(&self.codeword)
        .structure(&self.range);
    }
    fn load(&mut self, _: usize, r: &mut Reader) -> Result<(), Error> {
        self.attribs = r.list(|r| r.string())?;
        self.game_mode = r.string()?;
        self.min_participants = r.string()?;
        self.max_participants = r.string()?;
        self.matchmake_system = r.string()?;
        self.vacant_only = r.bool()?;
        self.exclude_locked = r.bool()?;
        self.exclude_non_host_pid = r.bool()?;
        self.selection_method = r.u32()?;
        self.vacant_participants = r.u16()?;
        self.params = r.structure::<MatchmakeParam>()?.0;
        self.exclude_user_password = r.bool()?;
        self.exclude_system_password = r.bool()?;
        self.refer_gid = r.u32()?;
        self.codeword = r.string()?;
        self.range = r.structure()?;
        Ok(())
    }
}

/// AutoMatchmakeWithParam_Postpone's parameter: join a session like `session` that passes
/// one of the criteria, or open it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AutoMatchmakeParam {
    pub session: MatchmakeSession,
    pub additional_participants: Vec<u64>,
    pub gid_for_participation_check: u32,
    pub option: u32,
    pub join_message: String,
    pub participation_count: u16,
    pub criteria: Vec<SearchCriteria>,
    pub target_gids: Vec<u32>,
    pub block_list_option: u32,
}

impl Structure for AutoMatchmakeParam {
    fn save(&self, _: usize, w: &mut Writer) {
        w.structure(&self.session)
            .list(&self.additional_participants, |w, p| {
                w.pid(*p);
            })
            .u32(self.gid_for_participation_check)
            .u32(self.option)
            .string(&self.join_message)
            .u16(self.participation_count)
            .list(&self.criteria, |w, c| {
                w.structure(c);
            })
            .list(&self.target_gids, |w, g| {
                w.u32(*g);
            })
            .structure(&BlockList(self.block_list_option));
    }
    fn load(&mut self, _: usize, r: &mut Reader) -> Result<(), Error> {
        self.session = r.structure()?;
        self.additional_participants = r.list(|r| r.pid())?;
        self.gid_for_participation_check = r.u32()?;
        self.option = r.u32()?;
        self.join_message = r.string()?;
        self.participation_count = r.u16()?;
        self.criteria = r.list(|r| r.structure())?;
        self.target_gids = r.list(|r| r.u32())?;
        if !r.is_empty() {
            self.block_list_option = r.structure::<BlockList>()?.0;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
struct BlockList(u32);

impl Structure for BlockList {
    fn save(&self, _: usize, w: &mut Writer) {
        w.u32(self.0);
    }
    fn load(&mut self, _: usize, r: &mut Reader) -> Result<(), Error> {
        self.0 = r.u32()?;
        Ok(())
    }
}

/// CreateMatchmakeSessionWithParam's parameter.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CreateMatchmakeSessionParam {
    pub session: MatchmakeSession,
    pub additional_participants: Vec<u64>,
    pub gid_for_participation_check: u32,
    pub option: u32,
    pub join_message: String,
    pub participation_count: u16,
}

impl Structure for CreateMatchmakeSessionParam {
    fn save(&self, _: usize, w: &mut Writer) {
        w.structure(&self.session)
            .list(&self.additional_participants, |w, p| {
                w.pid(*p);
            })
            .u32(self.gid_for_participation_check)
            .u32(self.option)
            .string(&self.join_message)
            .u16(self.participation_count);
    }
    fn load(&mut self, _: usize, r: &mut Reader) -> Result<(), Error> {
        self.session = r.structure()?;
        self.additional_participants = r.list(|r| r.pid())?;
        self.gid_for_participation_check = r.u32()?;
        self.option = r.u32()?;
        self.join_message = r.string()?;
        self.participation_count = r.u16()?;
        Ok(())
    }
}

/// JoinMatchmakeSessionWithParam's parameter.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct JoinMatchmakeSessionParam {
    pub gid: u32,
    pub additional_participants: Vec<u64>,
    pub gid_for_participation_check: u32,
    pub option: u32,
    pub behavior: u8,
    pub user_password: String,
    pub system_password: String,
    pub join_message: String,
    pub participation_count: u16,
    pub extra_participants: u16,
}

impl Structure for JoinMatchmakeSessionParam {
    fn save(&self, _: usize, w: &mut Writer) {
        w.u32(self.gid)
            .list(&self.additional_participants, |w, p| {
                w.pid(*p);
            })
            .u32(self.gid_for_participation_check)
            .u32(self.option)
            .u8(self.behavior)
            .string(&self.user_password)
            .string(&self.system_password)
            .string(&self.join_message)
            .u16(self.participation_count)
            .u16(self.extra_participants)
            .structure(&BlockList(0));
    }
    fn load(&mut self, _: usize, r: &mut Reader) -> Result<(), Error> {
        self.gid = r.u32()?;
        self.additional_participants = r.list(|r| r.pid())?;
        self.gid_for_participation_check = r.u32()?;
        self.option = r.u32()?;
        self.behavior = r.u8()?;
        self.user_password = r.string()?;
        self.system_password = r.string()?;
        self.join_message = r.string()?;
        self.participation_count = r.u16()?;
        self.extra_participants = r.u16()?;
        Ok(())
    }
}

/// A server push (Notifications.ProcessNotificationEvent).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NotificationEvent {
    pub source: u64,
    pub kind: u32,
    pub param1: u64,
    pub param2: u64,
    pub text: String,
    pub param3: u64,
}

impl Structure for NotificationEvent {
    fn save(&self, _: usize, w: &mut Writer) {
        w.pid(self.source)
            .u32(self.kind)
            .u64(self.param1)
            .u64(self.param2)
            .string(&self.text)
            .u64(self.param3);
    }
    fn load(&mut self, _: usize, r: &mut Reader) -> Result<(), Error> {
        self.source = r.pid()?;
        self.kind = r.u32()?;
        self.param1 = r.u64()?;
        self.param2 = r.u64()?;
        self.text = r.string()?;
        self.param3 = r.u64()?;
        Ok(())
    }
}

/// One entry of MatchMaking.GetDetailedParticipants.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ParticipantDetails {
    pub pid: u64,
    pub name: String,
    pub message: String,
    pub participants: u16,
}

impl Structure for ParticipantDetails {
    fn save(&self, _: usize, w: &mut Writer) {
        w.pid(self.pid)
            .string(&self.name)
            .string(&self.message)
            .u16(self.participants);
    }
    fn load(&mut self, _: usize, r: &mut Reader) -> Result<(), Error> {
        self.pid = r.pid()?;
        self.name = r.string()?;
        self.message = r.string()?;
        self.participants = r.u16()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::hex;

    // The same session and notification serialised by nextendo-nex (StreamOut.Add).
    #[test]
    fn layouts_match_the_server() {
        let s = MatchmakeSession {
            id: 1,
            owner: 5,
            host: 5,
            min_participants: 2,
            max_participants: 4,
            description: "d".into(),
            game_mode: smcraft::GAME_MODE,
            attribs: vec![1, 2],
            open_participation: true,
            application_data: b"SMC".to_vec(),
            session_key: vec![1],
            params: vec![("@SR".into(), Variant::Bool(true))],
            user_password: "pw".into(),
            codeword: "cw".into(),
            ..Default::default()
        };
        let mut w = Writer::new();
        w.structure(&s);
        assert_eq!(
            hex(w.bytes()),
            "002c00000001000000050000000000000005000000000000000200040000000000000000000000000000\
             00000002006400005300000043530000020000000100000002000000010000000003000000534d430000\
             000000010000000100000000000c00000001000000040040535200030100000000000000000300707700\
             0000000000000300637700"
        );
        assert_eq!(
            Reader::new(w.bytes())
                .structure::<MatchmakeSession>()
                .unwrap(),
            s
        );

        let n = NotificationEvent {
            source: 5,
            kind: NOTIFY_PARTICIPATE,
            param1: 1,
            param2: 6,
            text: String::new(),
            param3: 2,
        };
        let mut w = Writer::new();
        w.structure(&n);
        assert_eq!(
            hex(w.bytes()),
            "00270000000500000000000000b90b000001000000000000000600000000000000010000020000000000\
             0000"
        );
    }

    #[test]
    fn params_round_trip_and_smcraft_marks() {
        let s = MatchmakeSession::smcraft(smcraft::KIND_COOP, 2, 0x1122_3344_5566_7788);
        assert!(s.is_smcraft());
        assert_eq!(s.smcraft_course(), Some(0x1122_3344_5566_7788));
        let p = AutoMatchmakeParam {
            session: s.clone(),
            criteria: vec![SearchCriteria::smcraft(smcraft::KIND_COOP, 7)],
            participation_count: 1,
            ..Default::default()
        };
        let mut w = Writer::new();
        w.structure(&p);
        assert_eq!(
            Reader::new(w.bytes())
                .structure::<AutoMatchmakeParam>()
                .unwrap(),
            p
        );
        let j = JoinMatchmakeSessionParam {
            gid: 9,
            join_message: "hi".into(),
            ..Default::default()
        };
        let mut w = Writer::new();
        w.structure(&j);
        assert_eq!(
            Reader::new(w.bytes())
                .structure::<JoinMatchmakeSessionParam>()
                .unwrap(),
            j
        );
        let mut console = s;
        console.game_mode = 11;
        assert!(!console.is_smcraft());
    }
}
