//! SMM1's built-in sample courses for the course library (importer 7; DECISIONS 2026-10-25,
//! research/specs/smm1-course-import.md §9): the courses SMM1 offers as its sample courses,
//! converted once at import from the owner's own SMM1 into SMM2 course data
//! ([`crate::smm1_course`]), never shipped.
//!
//! - The list is SMM1's `Mush/SampleCourseDB.byaml` (`Pack/Static.pack`): 100 rows of
//!   `file_name` (`sample_course_data(_sub)_NN.cdt`), `ListNumber` (1–100), `MessageLabel`
//!   (`Msg_Smp_NNNN`), `type`, `scene`, `skin`, `priority` [obs data]. A row's course is the
//!   built-in `Course/sample_course_data_NN.szs` with its `_sub` (update first); the owner's v208
//!   has 60 of the 100 [obs data].
//! - Its name is the label's text in `Message/CommonMsg_<lang>.szs` → `Lcl_SampleCourseName`
//!   (the file's own name is a short working title) [obs data]; the converted course carries
//!   it as its name.
//!
//! Output: `courses/smm1_sample_NN.course` (raw SMM2 course data; the library lists them as
//! built-in courses, collection "SMM1 Sample") and `courses/samples.json` — `[{ "file", "list",
//! "label", "name", "type", "scene", "skin", "lossless" }]` in list order.

use std::path::Path;

use mmk_formats::archive::{self, Sarc};
use mmk_formats::byml;
use mmk_formats::msbt::Msbt;
use serde_json::{Value, json};

use crate::smm1::Overlay;
use crate::smm1_course;
use crate::{Progress, Result, write};

/// The library's file name prefix for SMM1's sample courses (its collection "SMM1 Sample").
pub const FILE_PREFIX: &str = "smm1_sample_";

#[derive(Clone, Debug, Default)]
pub struct SamplesReport {
    pub courses: usize,
    pub skipped: Vec<String>,
}

/// One `SampleCourseDB` row.
#[derive(Clone, Debug, PartialEq)]
pub struct SampleRow {
    /// The `NN` of `sample_course_data_NN`.
    pub number: u32,
    pub list: u32,
    pub label: String,
    pub kind: String,
    pub scene: String,
    pub skin: String,
}

/// The rows of a decoded `SampleCourseDB`, in list order.
pub fn rows(db: &Value) -> Vec<SampleRow> {
    let s = |r: &Value, k: &str| r[k].as_str().unwrap_or_default().to_string();
    let mut out: Vec<SampleRow> = db
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|r| {
            let file = r["file_name"].as_str()?;
            let number = file
                .strip_suffix(".cdt")?
                .rsplit('_')
                .next()?
                .parse()
                .ok()?;
            Some(SampleRow {
                number,
                list: r["ListNumber"].as_u64().unwrap_or(0) as u32,
                label: s(r, "MessageLabel"),
                kind: s(r, "type"),
                scene: s(r, "scene"),
                skin: s(r, "skin"),
            })
        })
        .collect();
    out.sort_by_key(|r| r.list);
    out
}

/// The sample courses' names: `Lcl_SampleCourseName` of the language's common messages.
fn names(ov: &Overlay, language: &str) -> Option<Msbt> {
    let path = ov.file(&format!("Message/CommonMsg_{language}.szs"))?;
    let data = archive::unwrap(std::fs::read(path).ok()?).ok()?;
    let sarc = Sarc::parse(data).ok()?;
    Msbt::parse(sarc.get("Lcl_SampleCourseName.msbt")?).ok()
}

pub(crate) fn import_samples(
    ov: &Overlay,
    out: &Path,
    language: &str,
    progress: Progress,
) -> Result<SamplesReport> {
    let mut report = SamplesReport::default();
    let Some(db) = ov
        .pack_member("Pack/Static.pack", "Mush/SampleCourseDB.byaml")
        .and_then(|d| byml::parse(&d).ok())
    else {
        report
            .skipped
            .push("Pack/Static.pack: no Mush/SampleCourseDB.byaml".into());
        return Ok(report);
    };
    let names = names(ov, language);
    if names.is_none() {
        report
            .skipped
            .push(format!("CommonMsg_{language}: no Lcl_SampleCourseName"));
    }
    let rows = rows(&db);
    let dir = out.join("courses");
    let mut index = Vec::new();
    for (i, r) in rows.iter().enumerate() {
        progress("sample courses", i, rows.len());
        let stem = format!("sample_course_data_{:02}", r.number);
        let Some(main) = ov.file(&format!("Course/{stem}.szs")) else {
            continue; // not in this build
        };
        let sub = ov.file(&format!("Course/{stem}_sub.szs"));
        let converted = smm1_course::read_area(&main).and_then(|m| {
            let s = sub.as_deref().map(smm1_course::read_area).transpose()?;
            smm1_course::convert(&m, s.as_ref())
        });
        let mut c = match converted {
            Ok(c) => c,
            Err(e) => {
                report.skipped.push(format!("{stem}: {e}"));
                continue;
            }
        };
        let name = names
            .as_ref()
            .and_then(|m| m.get(&r.label))
            .map(|m| m.plain())
            .filter(|n| !n.is_empty())
            .unwrap_or(c.name.clone());
        c.course.header.name.set(&name);
        let bytes = match c.course.to_bytes() {
            Ok(b) => b,
            Err(e) => {
                report.skipped.push(format!("{stem}: {e}"));
                continue;
            }
        };
        let file = format!("{FILE_PREFIX}{:02}.course", r.number);
        write(&dir.join(&file), &bytes)?;
        index.push(json!({
            "file": file, "list": r.list, "label": r.label, "name": name, "type": r.kind,
            "scene": r.scene, "skin": r.skin, "lossless": c.losses.is_empty(),
        }));
        report.courses += 1;
    }
    progress("sample courses", rows.len(), rows.len());
    if !index.is_empty() {
        write(
            &dir.join("samples.json"),
            serde_json::to_string_pretty(&Value::Array(index))
                .unwrap()
                .as_bytes(),
        )?;
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_come_in_list_order_with_their_file_numbers() {
        let db = json!([
            {"ListNumber": 2, "MessageLabel": "Msg_Smp_0002", "file_name": "sample_course_data(_sub)_01.cdt",
             "priority": "0", "scene": "Plain", "skin": "WU", "type": "2Short"},
            {"ListNumber": 1, "MessageLabel": "Msg_Smp_0052", "file_name": "sample_course_data(_sub)_51.cdt",
             "priority": "0", "scene": "Under", "skin": "M1", "type": "1Easy"},
            {"ListNumber": 3, "file_name": "not a course"},
        ]);
        let r = rows(&db);
        assert_eq!(r.len(), 2);
        assert_eq!(
            (r[0].number, r[0].list, r[0].label.as_str()),
            (51, 1, "Msg_Smp_0052")
        );
        assert_eq!((r[1].number, r[1].skin.as_str()), (1, "WU"));
    }

    /// The owner's SMM1 (skipped without the dump): the 60 sample courses of v208 convert,
    /// named from the game's text.
    #[test]
    fn owner_sample_courses_import() {
        let home = std::env::var_os("HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_default();
        let base = home.join("smm-work/extracted/smm1-disc/GM000500001018DC00000000000100/content");
        let update = home.join("smm-work/extracted/smm1-update-v208/content");
        if !base.is_dir() || !update.is_dir() {
            eprintln!("SMM1 dump not present; skipping");
            return;
        }
        let tmp = std::env::temp_dir().join(format!("mmk-smm1-samples-{}", std::process::id()));
        let cfg = crate::smm1::Smm1Config {
            base,
            update: Some(update),
            out: tmp.clone(),
            language: "USen".into(),
        };
        let ov = Overlay::new(&cfg);
        let report = import_samples(&ov, &tmp, "USen", &mut |_, _, _| {}).unwrap();
        assert_eq!(report.courses, 60, "{:?}", report.skipped);
        let index: Value =
            serde_json::from_slice(&std::fs::read(tmp.join("courses/samples.json")).unwrap())
                .unwrap();
        let first = &index[0];
        assert_eq!(first["list"], 1);
        let file = first["file"].as_str().unwrap();
        let c = mmk_formats::smm2::Course::parse(
            &std::fs::read(tmp.join("courses").join(file)).unwrap(),
        )
        .unwrap();
        assert_eq!(
            c.header.name.to_string_lossy(),
            first["name"].as_str().unwrap()
        );
        assert!(!c.header.name.to_string_lossy().is_empty());
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
