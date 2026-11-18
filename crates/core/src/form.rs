//! A form's fields taken together: what each holds, what is wrong with each, and
//! whether the whole may be submitted.

/// One field of a form.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Field {
    name: String,
    value: Vec<String>,
    initial: Vec<String>,
    required: bool,
    /// What the owner or the server says is wrong, whatever the value.
    error: Option<String>,
}

/// What stops a form being submitted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Invalid {
    /// The first field with a problem, which should take focus.
    pub first: String,
    /// Every field with a problem, and the problem.
    pub problems: Vec<(String, String)>,
}

#[derive(Clone, Debug, Default)]
pub struct Form {
    fields: Vec<Field>,
    /// Whether a submit has been tried, after which problems are shown as they arise.
    tried: bool,
}

impl Form {
    /// Adds a field with the value it starts with. A reset returns to it.
    #[must_use]
    pub fn field(mut self, name: &str, initial: &[&str], required: bool) -> Self {
        let value: Vec<String> = initial
            .iter()
            .map(std::string::ToString::to_string)
            .collect();
        self.fields.push(Field {
            name: name.into(),
            value: value.clone(),
            initial: value,
            required,
            error: None,
        });
        self
    }

    /// Records a field's value, as its primitive's gate reports it.
    pub fn set(&mut self, name: &str, value: Vec<String>) {
        if let Some(field) = self.fields.iter_mut().find(|field| field.name == name) {
            field.value = value;
            // A problem reported from outside is about the old value.
            field.error = None;
        }
    }

    /// Marks a field as wrong, as when the server refuses it.
    pub fn reject(&mut self, name: &str, problem: &str) {
        if let Some(field) = self.fields.iter_mut().find(|field| field.name == name) {
            field.error = Some(problem.to_string());
        }
    }

    #[must_use]
    pub fn value(&self, name: &str) -> &[String] {
        self.fields
            .iter()
            .find(|field| field.name == name)
            .map_or(&[], |field| &field.value)
    }

    /// What is wrong with a field, if anything.
    fn problem(field: &Field) -> Option<String> {
        let empty = field.value.iter().all(|entry| entry.trim().is_empty());
        field
            .error
            .clone()
            .or_else(|| (field.required && empty).then(|| "This field is required.".to_string()))
    }

    /// The problem to show beside a field. Nothing is shown until a submit has been
    /// tried, except what was reported from outside.
    #[must_use]
    pub fn error(&self, name: &str) -> Option<String> {
        let field = self.fields.iter().find(|field| field.name == name)?;
        if self.tried {
            Self::problem(field)
        } else {
            field.error.clone()
        }
    }

    /// Whether any field differs from what it started with.
    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.fields.iter().any(|field| field.value != field.initial)
    }

    /// The fields and their values if nothing is wrong, or what is wrong.
    pub fn submit(&mut self) -> Result<Vec<(String, Vec<String>)>, Invalid> {
        self.tried = true;
        let problems: Vec<(String, String)> = self
            .fields
            .iter()
            .filter_map(|field| Some((field.name.clone(), Self::problem(field)?)))
            .collect();
        match problems.first() {
            Some((first, _)) => Err(Invalid {
                first: first.clone(),
                problems,
            }),
            None => Ok(self
                .fields
                .iter()
                .map(|field| (field.name.clone(), field.value.clone()))
                .collect()),
        }
    }

    /// Puts every field back to what it started with.
    pub fn reset(&mut self) {
        self.tried = false;
        for field in &mut self.fields {
            field.value = field.initial.clone();
            field.error = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn form() -> Form {
        Form::default()
            .field("name", &[], true)
            .field("plan", &["free"], true)
            .field("notes", &[], false)
    }

    #[test]
    fn a_submit_names_every_problem_and_the_first_field_to_fix() {
        let mut form = form();
        assert_eq!(
            form.error("name"),
            None,
            "nothing is shown before a submit is tried"
        );
        let refused = form.submit().unwrap_err();
        assert_eq!(refused.first, "name");
        assert_eq!(refused.problems.len(), 1);
        assert_eq!(
            form.error("name").as_deref(),
            Some("This field is required.")
        );
        form.set("name", vec!["Ada".into()]);
        assert_eq!(form.error("name"), None);
        let sent = form.submit().unwrap();
        assert_eq!(sent[0], ("name".to_string(), vec!["Ada".to_string()]));
    }

    #[test]
    fn a_problem_from_outside_shows_until_the_value_changes_and_reset_goes_back() {
        let mut form = form();
        form.set("name", vec!["ada".into()]);
        form.reject("name", "That name is taken.");
        assert_eq!(form.error("name").as_deref(), Some("That name is taken."));
        assert!(form.submit().is_err());
        form.set("name", vec!["ada2".into()]);
        assert!(form.submit().is_ok());
        assert!(form.is_dirty());
        form.reset();
        assert!(!form.is_dirty());
        assert!(form.value("name").is_empty());
    }
}
