//! XLNK version 0x15 (big-endian): the Wii U SLink2DB of SMM1 (`Pack/Static.pack` →
//! `SLink2/SLink2DB.szs` → `SLink2DB.bslnk`). Same idea as SMM2's [`crate::xlink`] (users, each
//! with event keys → sound assets or containers, and action triggers), but laid out differently:
//! users are listed by name, calls are 24 bytes, asset parameters are not masked, conditions
//! carry no container kind. Read into the same [`Call`] / [`ActionTrigger`] types.
//!
//! Layout, from inspection of the owner's v208 file (research/specs/smm1-sound.md):
//!
//! ```text
//! 0x00  "XLNK" u32 size  u32 version (0x15)
//! 0x0C  u32 user param count, asset param count, ?, trigger param count
//! 0x1C  u32 asset param words (the asset param table is 4 × this long)
//! 0x20  u32 local property names, local property enum names, ?, direct values, random ranges,
//!       curves, curve points
//! 0x3C  u32 end of the value tables  u32 user count  u32 condition table pos  u32 name table pos
//! 0x4C  (u32 user offset, u32 user name pos) × users
//!       param defines: (u32 name pos, u32 type) × (user + asset + trigger params)
//!       asset param table: per asset one u32 per asset param, kind << 24 | value (kind 0:
//!       index into the direct value table, 1: name pos, 2: curve, 3: random range)
//!       …, local property name refs, then the value tables, which end at 0x3C: direct values
//!       (u32 float or int), random ranges (f32 min, max), curves (20 bytes as in v0x1C), points
//! user: 11 × i32 header ([1] calls, [4] action slots, [5] actions, [6] action triggers, [10]
//!       trigger table offset from the user), u32 user params × user param count, u16 call sort
//!       table × calls (padded to 4), then 24-byte calls: u32 key name pos, i16 asset index (−1:
//!       container), u16 flag, i32 duration, i32 parent call, u32 param pos, i32 condition pos
//!       (−1: none). An asset's param pos is an offset into the asset param table; a container's
//!       is an offset from the end of the call table to u32 kind (0 switch, 1 random, 2 random2,
//!       3 blend, 4 sequence), i32 first child, i32 last child, and for a switch u32 property
//!       name pos, i32 property id, i16 local property index, u8 global, u8.
//! condition (condition table pos + the call's condition pos), by the parent container's kind:
//!       switch: u32 property type (0 enum, 1 int, 2 float), u32 compare (0 ==, 1 >, 2 >=, 3 <,
//!       4 <=, 5 !=), u32 value (enum: name pos), i16 enum index, u8, u8 global;
//!       random: f32 weight.
//! trigger table: action slots (u32 name pos, u16 first action, u16 last action), actions
//!       (u32 name pos, u32 first trigger, u32 last trigger), triggers (24 bytes: u32 guid,
//!       u32 call offset (24-byte calls), i32 start frame, i32 end frame (0x7FFFFFFF: none),
//!       u32 flags, i32 overwrite param pos)
//! ```
//!
//! Names are stored in at most 23 bytes (longer action names are cut, e.g.
//! `State::cState_TouchSele`); property names are Shift-JIS.

use std::collections::BTreeMap;

use crate::error::{Error, Result};
use crate::xlink::{ActionTrigger, Call, CondValue, Condition, ContainerKind, Curve, Param};

fn bad(detail: impl Into<String>) -> Error {
    Error::Format {
        what: "XLNK v0x15",
        detail: detail.into(),
    }
}

fn u32_at(d: &[u8], o: usize) -> Result<u32> {
    d.get(o..o + 4)
        .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
        .ok_or_else(|| bad(format!("truncated at {o:#x}")))
}

fn u16_at(d: &[u8], o: usize) -> Result<u16> {
    d.get(o..o + 2)
        .map(|b| u16::from_be_bytes([b[0], b[1]]))
        .ok_or_else(|| bad(format!("truncated at {o:#x}")))
}

fn i32_at(d: &[u8], o: usize) -> Result<i32> {
    u32_at(d, o).map(|v| v as i32)
}

fn f32_at(d: &[u8], o: usize) -> Result<f32> {
    u32_at(d, o).map(f32::from_bits)
}

/// Header size; the user table starts here.
const USERS: usize = 0x4C;
const USER_HEADER: usize = 44;
const CALL: usize = 24;

pub struct XlnkWiiU<'a> {
    d: &'a [u8],
    users: BTreeMap<String, usize>,
    name_pos: usize,
    cond_pos: usize,
    n_user_params: usize,
    asset_params: Vec<(String, u32)>,
    asset_base: usize,
    asset_end: usize,
    direct_base: usize,
    random_base: usize,
    curve_base: usize,
    point_base: usize,
    n_direct: usize,
    n_random: usize,
    n_curve: usize,
    n_point: usize,
}

impl<'a> XlnkWiiU<'a> {
    pub fn parse(d: &'a [u8]) -> Result<Self> {
        if d.len() < USERS || &d[0..4] != b"XLNK" {
            return Err(Error::Magic { what: "XLNK" });
        }
        let version = u32_at(d, 8)?;
        if version != 0x15 {
            return Err(bad(format!("unsupported version {version:#x}")));
        }
        let count = |o: usize| -> Result<usize> { Ok(u32_at(d, o)? as usize) };
        let (nu, na, nt) = (count(0x0C)?, count(0x10)?, count(0x18)?);
        let asset_words = count(0x1C)?;
        let (n_direct, n_random, n_curve, n_point) =
            (count(0x2C)?, count(0x30)?, count(0x34)?, count(0x38)?);
        let values_end = count(0x3C)?;
        let n_users = count(0x40)?;
        let cond_pos = count(0x44)?;
        let name_pos = count(0x48)?;
        if name_pos >= d.len() || n_users > d.len() / 8 || nu + na + nt > 4096 {
            return Err(bad("header out of range"));
        }
        let name = |o: u32| cstr(d, name_pos + o as usize);
        let mut users = BTreeMap::new();
        for i in 0..n_users {
            let at = USERS + 8 * i;
            users.insert(name(u32_at(d, at + 4)?)?, u32_at(d, at)? as usize);
        }
        let defs = USERS + 8 * n_users;
        let define = |i: usize| -> Result<(String, u32)> {
            Ok((
                name(u32_at(d, defs + 8 * i)?)?,
                u32_at(d, defs + 8 * i + 4)?,
            ))
        };
        let asset_params = (nu..nu + na).map(define).collect::<Result<Vec<_>>>()?;
        let asset_base = defs + 8 * (nu + na + nt);
        // the value tables, back from their end
        let point_base = values_end
            .checked_sub(8 * n_point)
            .ok_or_else(|| bad("value tables"))?;
        let curve_base = point_base
            .checked_sub(20 * n_curve)
            .ok_or_else(|| bad("value tables"))?;
        let random_base = curve_base
            .checked_sub(8 * n_random)
            .ok_or_else(|| bad("value tables"))?;
        let direct_base = random_base
            .checked_sub(4 * n_direct)
            .ok_or_else(|| bad("value tables"))?;
        let asset_end = asset_base + 4 * asset_words;
        if asset_end > direct_base || values_end > d.len() {
            return Err(bad("value tables overlap the asset params"));
        }
        Ok(Self {
            d,
            users,
            name_pos,
            cond_pos,
            n_user_params: nu,
            asset_params,
            asset_base,
            asset_end,
            direct_base,
            random_base,
            curve_base,
            point_base,
            n_direct,
            n_random,
            n_curve,
            n_point,
        })
    }

    fn name(&self, o: u32) -> Result<String> {
        cstr(self.d, self.name_pos + o as usize)
    }

    /// The users' names (sorted).
    pub fn user_names(&self) -> impl Iterator<Item = &str> {
        self.users.keys().map(String::as_str)
    }

    pub fn has_user(&self, name: &str) -> bool {
        self.users.contains_key(name)
    }

    fn header(&self, u: usize) -> Result<[usize; 11]> {
        let mut h = [0; 11];
        for (k, v) in h.iter_mut().enumerate() {
            *v = i32_at(self.d, u + 4 * k)?.max(0) as usize;
        }
        Ok(h)
    }

    /// Where user `u`'s call table starts.
    fn calls_at(&self, u: usize, n_call: usize) -> usize {
        let p = u + USER_HEADER + 4 * self.n_user_params + 2 * n_call;
        (p + 3) & !3
    }

    /// The calls of user `name` (None if there is no such user).
    pub fn user_calls(&self, name: &str) -> Result<Option<Vec<Call>>> {
        let Some(&u) = self.users.get(name) else {
            return Ok(None);
        };
        let d = self.d;
        let n_call = self.header(u)?[1];
        let p = self.calls_at(u, n_call);
        let ex = p + CALL * n_call;
        // kinds first: a child's condition is read by its parent's kind
        let raw: Vec<(u32, i16, i32, u32, i32)> = (0..n_call)
            .map(|i| {
                let o = p + CALL * i;
                let asset = u16_at(d, o + 4)? as i16;
                Ok((
                    u32_at(d, o)?,
                    asset,
                    i32_at(d, o + 12)?,
                    u32_at(d, o + 16)?,
                    i32_at(d, o + 20)?,
                ))
            })
            .collect::<Result<_>>()?;
        let kind_of = |i: usize| -> Result<Option<ContainerKind>> {
            let (_, asset, _, pp, _) = raw[i];
            if asset >= 0 {
                return Ok(None);
            }
            Ok(Some(container_kind(u32_at(d, ex + pp as usize)?)))
        };
        let mut out = Vec::with_capacity(n_call);
        for (i, &(key, asset, parent, pp, cond)) in raw.iter().enumerate() {
            let key = self.name(key)?;
            let (mut kind, mut children, mut watch) = (None, None, None);
            let params = if asset < 0 {
                let c = ex + pp as usize;
                let k = container_kind(u32_at(d, c)?);
                let (a, b) = (i32_at(d, c + 4)?, i32_at(d, c + 8)?);
                if a >= 0 && b >= a && (b as usize) < n_call {
                    children = Some((a as usize, b as usize));
                }
                if k == ContainerKind::Switch {
                    let prop =
                        sjis_lossy(&cstr_bytes(d, self.name_pos + u32_at(d, c + 12)? as usize)?);
                    let global = d.get(c + 22).copied().unwrap_or(0) != 0;
                    watch = Some((prop, global));
                }
                kind = Some(k);
                BTreeMap::new()
            } else {
                self.params(pp as usize)?
            };
            let parent_kind = match usize::try_from(parent) {
                Ok(p) if p < n_call && p != i => kind_of(p)?,
                _ => None,
            };
            let condition = if cond >= 0 {
                Some(self.condition(cond as usize, parent_kind)?)
            } else {
                None
            };
            out.push(Call {
                key,
                params,
                container: asset < 0,
                parent: (parent >= 0).then_some(parent as usize),
                conditional: cond >= 0,
                kind,
                children,
                watch,
                condition,
            });
        }
        Ok(Some(out))
    }

    /// The action triggers of user `name`: entering action `action` (a state of the game's UI
    /// object, e.g. `State::cState_Walk`) starts call `call` at `start_frame`.
    pub fn user_actions(&self, name: &str) -> Result<Option<Vec<ActionTrigger>>> {
        let Some(&u) = self.users.get(name) else {
            return Ok(None);
        };
        let d = self.d;
        let h = self.header(u)?;
        let (n_call, n_slot, n_act, n_trig) = (h[1], h[4], h[5], h[6]);
        let mut t = u + h[10];
        let mut slots = Vec::with_capacity(n_slot);
        for _ in 0..n_slot {
            slots.push((
                self.name(u32_at(d, t)?)?,
                u16_at(d, t + 4)? as usize,
                u16_at(d, t + 6)? as usize,
            ));
            t += 8;
        }
        let mut acts = Vec::with_capacity(n_act);
        for _ in 0..n_act {
            acts.push((
                self.name(u32_at(d, t)?)?,
                u32_at(d, t + 4)? as usize,
                u32_at(d, t + 8)? as usize,
            ));
            t += 12;
        }
        let mut trigs = Vec::with_capacity(n_trig);
        for _ in 0..n_trig {
            trigs.push((
                u32_at(d, t + 4)? as usize / CALL,
                i32_at(d, t + 8)?,
                i32_at(d, t + 12)?,
                u32_at(d, t + 16)?,
            ));
            t += 24;
        }
        let mut out = Vec::new();
        for (ai, (action, first, last)) in acts.iter().enumerate() {
            let slot = slots
                .iter()
                .find(|(_, a, b)| (*a..=*b).contains(&ai))
                .map(|s| s.0.clone())
                .unwrap_or_default();
            for &(call, start, end, flag) in trigs.get(*first..=*last).into_iter().flatten() {
                if call >= n_call {
                    continue;
                }
                out.push(ActionTrigger {
                    slot: slot.clone(),
                    action: action.clone(),
                    call,
                    start_frame: start,
                    end_frame: (end != i32::MAX).then_some(end),
                    after: None,
                    flag,
                });
            }
        }
        Ok(Some(out))
    }

    fn condition(&self, pos: usize, parent: Option<ContainerKind>) -> Result<Condition> {
        let d = self.d;
        let o = self.cond_pos + pos;
        Ok(match parent {
            Some(ContainerKind::Random | ContainerKind::Random2) => Condition::Random {
                weight: f32_at(d, o)?,
            },
            Some(ContainerKind::Switch) => {
                let ptype = u32_at(d, o)?;
                let cmp = crate::xlink::Compare::from_raw(u32_at(d, o + 4)?)
                    .unwrap_or(crate::xlink::Compare::Eq);
                let raw = u32_at(d, o + 8)?;
                let value = match ptype {
                    0 => CondValue::Enum(self.name(raw)?),
                    2 => CondValue::Float(f32::from_bits(raw)),
                    _ => CondValue::Int(raw as i32),
                };
                let global = d.get(o + 15).copied().unwrap_or(0) != 0;
                Condition::Switch { cmp, value, global }
            }
            _ => Condition::Other(vec![u32_at(d, o)?, u32_at(d, o + 4)?, u32_at(d, o + 8)?]),
        })
    }

    fn curve(&self, i: usize) -> Result<Curve> {
        if i >= self.n_curve {
            return Err(bad(format!("curve {i} out of range")));
        }
        let d = self.d;
        let o = self.curve_base + 20 * i;
        let first = u16_at(d, o)? as usize;
        let n = u16_at(d, o + 2)? as usize;
        let global = u16_at(d, o + 6)? != 0;
        let property = sjis_lossy(&cstr_bytes(d, self.name_pos + u32_at(d, o + 8)? as usize)?);
        let mut points = Vec::with_capacity(n);
        for k in first..(first + n).min(self.n_point) {
            let p = self.point_base + 8 * k;
            points.push((f32_at(d, p)?, f32_at(d, p + 4)?));
        }
        Ok(Curve {
            property,
            global,
            points,
        })
    }

    /// One encoded value (kind << 24 | value) of a parameter of define type `ptype`.
    fn value(&self, raw: u32, ptype: u32) -> Result<Param> {
        let (kind, value) = (raw >> 24, (raw & 0xFF_FFFF) as usize);
        Ok(match kind {
            0 if value < self.n_direct => {
                let bits = u32_at(self.d, self.direct_base + 4 * value)?;
                // define type 1 = float, else an integer
                if ptype == 1 {
                    Param::Num(f32::from_bits(bits))
                } else {
                    Param::Int(bits as i32)
                }
            }
            1 => Param::Str(self.name(value as u32)?),
            2 => Param::Curve(self.curve(value)?),
            3 if value < self.n_random => {
                let o = self.random_base + 8 * value;
                Param::Range(f32_at(self.d, o)?, f32_at(self.d, o + 4)?)
            }
            _ => Param::Raw(raw),
        })
    }

    fn params(&self, pos: usize) -> Result<BTreeMap<String, Param>> {
        let at = self.asset_base + pos;
        if at + 4 * self.asset_params.len() > self.asset_end {
            return Err(bad(format!("asset params at {pos:#x} out of range")));
        }
        let mut out = BTreeMap::new();
        for (k, (pname, ptype)) in self.asset_params.iter().enumerate() {
            let v = self.value(u32_at(self.d, at + 4 * k)?, *ptype)?;
            out.insert(pname.clone(), v);
        }
        Ok(out)
    }
}

fn container_kind(v: u32) -> ContainerKind {
    match v {
        0 => ContainerKind::Switch,
        1 => ContainerKind::Random,
        2 => ContainerKind::Random2,
        3 => ContainerKind::Blend,
        4 => ContainerKind::Sequence,
        o => ContainerKind::Other(o),
    }
}

fn cstr_bytes(d: &[u8], o: usize) -> Result<Vec<u8>> {
    let rest = d.get(o..).ok_or_else(|| bad("string past end"))?;
    let end = rest
        .iter()
        .position(|&b| b == 0)
        .ok_or_else(|| bad("unterminated string"))?;
    Ok(rest[..end].to_vec())
}

fn cstr(d: &[u8], o: usize) -> Result<String> {
    Ok(String::from_utf8_lossy(&cstr_bytes(d, o)?).into_owned())
}

/// A property name: kept when it is ASCII, else written as `sjis:<hex>` (the database's
/// property names are Shift-JIS; nothing here decodes them).
fn sjis_lossy(b: &[u8]) -> String {
    if b.is_ascii() {
        String::from_utf8_lossy(b).into_owned()
    } else {
        let hex: String = b.iter().map(|x| format!("{x:02x}")).collect();
        format!("sjis:{hex}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xlink::Compare;

    #[test]
    fn rejects_other_files() {
        assert!(XlnkWiiU::parse(b"NOPE").is_err());
        let mut d = vec![0u8; 0x50];
        d[0..4].copy_from_slice(b"XLNK");
        d[11] = 0x1C;
        assert!(XlnkWiiU::parse(&d).is_err());
    }

    #[test]
    fn reads_users_calls_switches_and_triggers() {
        let d = crate::fixtures::sample_xlnk_wiiu();
        let x = XlnkWiiU::parse(&d).unwrap();
        assert_eq!(x.user_names().collect::<Vec<_>>(), ["UI_L"]);
        assert!(x.user_calls("UI_M").unwrap().is_none());
        let calls = x.user_calls("UI_L").unwrap().unwrap();
        assert_eq!(calls.len(), 5);
        assert_eq!(calls[0].key, "Decide");
        assert_eq!(calls[0].params["RuntimeAssetName"].as_str(), Some("SE_Ok"));
        assert_eq!(calls[0].params["Volume"].as_f32(), Some(1.0));
        assert_eq!(calls[0].params["Delay"].as_f32(), Some(0.0));
        assert_eq!(calls[1].kind, Some(ContainerKind::Blend));
        assert_eq!(calls[1].children, Some((2, 2)));
        assert_eq!(calls[2].kind, Some(ContainerKind::Switch));
        assert_eq!(calls[2].watch, Some(("Count".into(), false)));
        assert_eq!(calls[2].parent, Some(1));
        assert_eq!(
            calls[3].condition,
            Some(Condition::Switch {
                cmp: Compare::Ge,
                value: CondValue::Int(50),
                global: false
            })
        );
        assert_eq!(calls[4].params["Volume"].as_f32(), Some(0.5));
        let acts = x.user_actions("UI_L").unwrap().unwrap();
        assert_eq!(acts.len(), 1);
        assert_eq!(
            (acts[0].slot.as_str(), acts[0].action.as_str()),
            ("cSlot_State", "State::cState_Walk")
        );
        assert_eq!((acts[0].call, acts[0].start_frame), (1, 5));
        assert_eq!(acts[0].end_frame, None);
    }

    /// The owner's SMM1 database, when present (skipped in CI).
    #[test]
    fn owner_smm1_database() {
        let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
        let Some(p) =
            home.map(|h| h.join("smm-work/extracted/smm1-update-v208/content/Pack/Static.pack"))
        else {
            return;
        };
        let Ok(data) = std::fs::read(&p) else {
            eprintln!("SMM1 dump not present; skipping");
            return;
        };
        let pack = crate::archive::Sarc::parse(crate::archive::unwrap(data).unwrap()).unwrap();
        let szs =
            crate::archive::unwrap(pack.get("SLink2/SLink2DB.szs").unwrap().to_vec()).unwrap();
        let db = crate::archive::Sarc::parse(szs).unwrap();
        let x = XlnkWiiU::parse(db.get("SLink2DB.bslnk").unwrap()).unwrap();
        assert!(x.user_names().count() > 1000);
        let user = "UI_Nwk_WorldMapEsy_00_L_WorldMapKuribo_00";
        let calls = x.user_calls(user).unwrap().unwrap();
        let walk = calls.iter().position(|c| c.key == "walk").unwrap();
        let acts = x.user_actions(user).unwrap().unwrap();
        let t = acts.iter().find(|a| a.call == walk).unwrap();
        assert_eq!(t.action, "State::cState_Walk");
        assert_eq!(t.end_frame, Some(300));
        let card = x
            .user_calls("UI_Play_CourseIn100Mario_00")
            .unwrap()
            .unwrap();
        let w80 = card.iter().find(|c| c.key == "CouseInWalk100_05").unwrap();
        assert_eq!(
            w80.condition,
            Some(Condition::Switch {
                cmp: Compare::Ge,
                value: CondValue::Int(80),
                global: false
            })
        );
        assert_eq!(w80.params["Delay"].as_f32(), Some(32.0));
    }
}
