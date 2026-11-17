use crate::agent::{ActError, Snapshot, SnapshotItem};
use crate::change::{Gate, Reason, Source, ValueChange};
use crate::keys::Key;
use crate::primitive::{
    Effect, FileInfo, Focusable, Input, Item, PartOf, Primitive, Reaction, Role, Semantics,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Part {
    /// Where files are dropped. Pressing it opens the file picker.
    Dropzone,
    /// A button that opens the file picker.
    Trigger,
    List,
    /// One chosen file, keyed by its name.
    File,
    /// Takes one file back out, keyed like its file.
    Remove,
}

/// Why a file was turned away.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rejection {
    WrongType,
    TooLarge,
    TooMany,
}

#[derive(Clone, Debug, Default)]
pub struct Options {
    pub multiple: bool,
    /// The types taken: extensions such as `.png`, media types such as
    /// `application/pdf`, or families such as `image/*`. Empty takes anything.
    pub accept: Vec<String>,
    /// The largest file taken, in bytes.
    pub max_size: Option<u64>,
    pub max_files: Option<usize>,
}

const ACTIONS: &[&str] = &["remove", "clear", "browse"];

/// Chooses files, from the platform's picker or by dropping them. Files of the
/// wrong type, too large, or past the limit are turned away and listed in
/// `rejected`. The value is the names of the files taken.
#[derive(Clone, Debug, Default)]
pub struct FileInput {
    opts: Options,
    files: Vec<FileInfo>,
    rejected: Vec<(String, Rejection)>,
    /// Whether files are being dragged over the dropzone.
    over: bool,
    focused: Option<(Part, Option<String>)>,
}

impl FileInput {
    pub fn new(opts: Options) -> Self {
        FileInput {
            opts,
            ..Default::default()
        }
    }

    pub fn files(&self) -> &[FileInfo] {
        &self.files
    }

    /// The files turned away the last time some were offered, and why.
    pub fn rejected(&self) -> &[(String, Rejection)] {
        &self.rejected
    }

    fn accepts(&self, file: &FileInfo) -> bool {
        let name = file.name.to_lowercase();
        self.opts.accept.is_empty()
            || self
                .opts
                .accept
                .iter()
                .map(|rule| rule.trim().to_lowercase())
                .any(|rule| match rule.strip_suffix("/*") {
                    Some(family) => file.kind.starts_with(&format!("{family}/")),
                    None if rule.starts_with('.') => name.ends_with(&rule),
                    None => file.kind == rule,
                })
    }

    fn written(files: &[FileInfo]) -> Vec<String> {
        files.iter().map(|file| file.name.clone()).collect()
    }

    fn commit(&mut self, files: Vec<FileInfo>, source: Source, reason: Reason, gate: Gate) -> bool {
        if Self::written(&files) == Self::written(&self.files) {
            return true;
        }
        let allowed = gate(&ValueChange {
            value: Self::written(&files),
            source,
            reason,
        });
        if allowed {
            self.files = files;
        }
        allowed
    }

    /// Sorts offered files into those taken and those turned away.
    fn offer(&mut self, offered: Vec<FileInfo>, reason: Reason, gate: Gate) -> Reaction<Part> {
        let mut taken = if self.opts.multiple {
            self.files.clone()
        } else {
            Vec::new()
        };
        let limit = if self.opts.multiple {
            self.opts.max_files.unwrap_or(usize::MAX)
        } else {
            1
        };
        let mut rejected = Vec::new();
        let mut added = 0;
        for file in offered {
            let problem = if !self.accepts(&file) {
                Some(Rejection::WrongType)
            } else if self.opts.max_size.is_some_and(|most| file.size > most) {
                Some(Rejection::TooLarge)
            } else if taken.len() >= limit {
                Some(Rejection::TooMany)
            } else {
                None
            };
            match problem {
                Some(problem) => rejected.push((file.name, problem)),
                // A file offered again replaces the one of the same name.
                None => {
                    taken.retain(|held| held.name != file.name);
                    taken.push(file);
                    added += 1;
                }
            }
        }
        if added > 0 && !self.commit(taken, Source::User, reason, gate) {
            return Reaction::handled();
        }
        let plural = |count: usize| if count == 1 { "file" } else { "files" };
        let mut message = format!("{added} {} added.", plural(added));
        if !rejected.is_empty() {
            message.push_str(&format!(
                " {} {} not accepted.",
                rejected.len(),
                plural(rejected.len())
            ));
        }
        self.rejected = rejected;
        Reaction::handled().announcing(message)
    }

    fn browse(&self) -> Reaction<Part> {
        Reaction::handled().with(Effect::PickFiles {
            multiple: self.opts.multiple,
            accept: self.opts.accept.join(","),
        })
    }
}

impl PartOf for Part {
    type Primitive = FileInput;
}

impl Primitive for FileInput {
    type Part = Part;

    fn parts() -> &'static [Part] {
        &[
            Part::Dropzone,
            Part::Trigger,
            Part::List,
            Part::File,
            Part::Remove,
        ]
    }

    fn is_item(_: Part) -> bool {
        false
    }

    fn drops(part: Part) -> bool {
        part == Part::Dropzone
    }

    fn named_part(&self) -> Option<Part> {
        Some(Part::Dropzone)
    }

    fn upsert_item(&mut self, _: Item) {}
    fn remove_item(&mut self, _: &str) {}
    fn set_order(&mut self, _: &[(String, String)]) {}

    fn semantics(&self, part: Part, item: Option<&str>) -> Semantics {
        let held = item.is_some_and(|name| self.files.iter().any(|file| file.name == name));
        match part {
            Part::Dropzone => Semantics {
                role: Some(Role::Button),
                focusable: Focusable::TabStop,
                active: self.over,
                ..Default::default()
            },
            Part::Trigger => Semantics {
                role: Some(Role::Button),
                focusable: Focusable::TabStop,
                ..Default::default()
            },
            Part::List => Semantics {
                role: Some(Role::List),
                hidden: self.files.is_empty(),
                ..Default::default()
            },
            Part::File => Semantics {
                role: Some(Role::ListItem),
                hidden: item.is_some() && !held,
                ..Default::default()
            },
            Part::Remove => Semantics {
                role: Some(Role::Button),
                hidden: item.is_some() && !held,
                focusable: if held {
                    Focusable::TabStop
                } else {
                    Focusable::No
                },
                ..Default::default()
            },
        }
    }

    fn send(&mut self, input: Input<Part>, gate: Gate) -> Reaction<Part> {
        match input {
            Input::Focus { part, item } => {
                self.focused = Some((part, item));
                Reaction::default()
            }
            Input::DragOver { inside } => {
                self.over = inside;
                Reaction::default()
            }
            Input::Files { files, dropped } => {
                self.over = false;
                self.offer(
                    files,
                    if dropped { Reason::Drag } else { Reason::Press },
                    gate,
                )
            }
            Input::Press {
                part: Part::Dropzone | Part::Trigger,
                ..
            } => self.browse(),
            Input::Press {
                part: Part::Remove,
                item: Some(name),
                ..
            } => {
                let rest = self
                    .files
                    .iter()
                    .filter(|file| file.name != name)
                    .cloned()
                    .collect();
                self.commit(rest, Source::User, Reason::Press, gate);
                Reaction::focusing(Part::Dropzone)
            }
            Input::Key {
                key: Key::Enter | Key::Space,
                mods,
                ..
            } => match self.focused.clone() {
                Some((part @ (Part::Dropzone | Part::Trigger | Part::Remove), item)) => {
                    self.send(Input::Press { part, item, mods }, gate)
                }
                _ => Reaction::default(),
            },
            _ => Reaction::default(),
        }
    }

    fn value(&self) -> Vec<String> {
        Self::written(&self.files)
    }

    /// The owner can take files away by name. It cannot put in files it does not hold.
    fn set_value(&mut self, value: Vec<String>) {
        self.files.retain(|file| value.contains(&file.name));
    }

    fn snapshot(&self, label: &str) -> Snapshot {
        let items = self
            .files
            .iter()
            .map(|file| SnapshotItem {
                key: file.name.clone(),
                label: file.name.clone(),
                disabled: false,
                selected: true,
            })
            .collect();
        Snapshot {
            name: "file-input",
            label: label.to_string(),
            value: self.value(),
            expanded: None,
            actions: ACTIONS,
            items,
        }
    }

    fn act(&mut self, action: &str, item: Option<&str>, gate: Gate) -> Result<(), ActError> {
        let rest = match action {
            // An agent cannot choose files from the person's disk. It can only ask.
            "browse" => return Err(ActError::Refused),
            "clear" => Vec::new(),
            "remove" => {
                let name = item.ok_or(ActError::MissingItem { action: "remove" })?;
                if !self.files.iter().any(|file| file.name == name) {
                    return Err(ActError::UnknownItem(name.to_string()));
                }
                self.files
                    .iter()
                    .filter(|file| file.name != name)
                    .cloned()
                    .collect()
            }
            other => {
                return Err(ActError::UnknownAction {
                    action: other.to_string(),
                    valid: ACTIONS,
                });
            }
        };
        if self.commit(rest, Source::Agent, Reason::Action, gate) {
            Ok(())
        } else {
            Err(ActError::Refused)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::Modifiers;

    fn file(name: &str, kind: &str, size: u64) -> FileInfo {
        FileInfo {
            name: name.into(),
            kind: kind.into(),
            size,
            path: None,
        }
    }

    fn images() -> FileInput {
        FileInput::new(Options {
            multiple: true,
            accept: vec!["image/*".into(), ".pdf".into()],
            max_size: Some(1000),
            max_files: Some(2),
        })
    }

    #[test]
    fn files_are_taken_or_turned_away_with_a_reason() {
        let mut input = images();
        let offered = vec![
            file("a.png", "image/png", 10),
            file("b.txt", "text/plain", 10),
            file("c.jpg", "image/jpeg", 5000),
            file("d.PDF", "", 10),
            file("e.png", "image/png", 10),
        ];
        let said = input.send(
            Input::Files {
                files: offered,
                dropped: true,
            },
            &mut |_| true,
        );
        assert_eq!(input.value(), ["a.png", "d.PDF"]);
        assert_eq!(
            input.rejected(),
            [
                ("b.txt".to_string(), Rejection::WrongType),
                ("c.jpg".to_string(), Rejection::TooLarge),
                ("e.png".to_string(), Rejection::TooMany)
            ]
        );
        assert_eq!(
            said.effects,
            [Effect::Announce {
                message: "2 files added. 3 files not accepted.".into(),
                assertive: false
            }]
        );
    }

    #[test]
    fn pressing_the_dropzone_asks_for_the_picker_and_a_single_input_keeps_the_latest() {
        let mut input = FileInput::new(Options {
            accept: vec![".csv".into()],
            ..Default::default()
        });
        let asked = input.send(
            Input::Press {
                part: Part::Dropzone,
                item: None,
                mods: Modifiers::default(),
            },
            &mut |_| true,
        );
        assert_eq!(
            asked.effects,
            [Effect::PickFiles {
                multiple: false,
                accept: ".csv".into()
            }]
        );
        input.send(
            Input::Files {
                files: vec![file("one.csv", "text/csv", 1)],
                dropped: false,
            },
            &mut |_| true,
        );
        input.send(
            Input::Files {
                files: vec![file("two.csv", "text/csv", 1)],
                dropped: false,
            },
            &mut |_| true,
        );
        assert_eq!(input.value(), ["two.csv"]);
    }

    #[test]
    fn a_file_can_be_taken_back_out_and_an_agent_cannot_browse() {
        let mut input = images();
        input.send(
            Input::Files {
                files: vec![file("a.png", "image/png", 10)],
                dropped: false,
            },
            &mut |_| true,
        );
        input.send(
            Input::Press {
                part: Part::Remove,
                item: Some("a.png".into()),
                mods: Modifiers::default(),
            },
            &mut |_| true,
        );
        assert!(input.value().is_empty());
        assert!(input.semantics(Part::List, None).hidden);
        assert_eq!(
            input.act("browse", None, &mut |_| true),
            Err(ActError::Refused)
        );
    }

    #[test]
    fn it_conforms() {
        crate::testing::assert_conforms(images, &[]);
    }
}
