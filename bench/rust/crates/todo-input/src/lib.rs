//! `<todo-input>`: the field for a new todo, and the control that marks
//! every todo complete or active. It reports a new title as `todo-add`
//! and the toggle as `todo-toggle-all`, and `<todo-app>` handles both.

use element_kit::{Element, Event, Host, View, attribute, h};

struct TodoInput {
    host: Host,
    has_todos: bool,
    all_completed: bool,
    /// Whether the next render empties the field. An add sets it, and
    /// anything else that happens to the element clears it, so the field
    /// empties in the one render after the add, and a later render, such
    /// as one for a changed attribute, leaves it to the person typing.
    clear: bool,
}

impl Element for TodoInput {
    const OBSERVED_ATTRIBUTES: &'static [&'static str] = &["has-todos", "all-completed"];
    const STYLES: &'static str = include_str!("styles.css");

    fn new(host: Host, attributes: &[(String, String)]) -> Self {
        TodoInput {
            host,
            has_todos: attribute(attributes, "has-todos") == Some("true"),
            all_completed: attribute(attributes, "all-completed") == Some("true"),
            clear: false,
        }
    }

    fn connected(&mut self) {
        self.clear = false;
    }

    fn attribute_changed(&mut self, name: &str, value: Option<&str>) {
        self.clear = false;
        match (name, value) {
            ("has-todos", Some(value)) => self.has_todos = value == "true",
            ("all-completed", Some(value)) => self.all_completed = value == "true",
            _ => {}
        }
    }

    fn handle_event(&mut self, handler: &str, event: Event) {
        self.clear = false;
        match (handler, event.key.as_deref(), event.value.as_deref()) {
            ("keydown", Some("Enter"), Some(text)) => {
                let title = text.trim();
                if !title.is_empty() {
                    self.host.emit("todo-add", title);
                    self.clear = true;
                }
            }
            ("toggle-all", _, _) => {
                let all = if event.checked.unwrap_or(false) { "true" } else { "false" };
                self.host.emit("todo-toggle-all", all);
            }
            _ => {}
        }
    }

    // Both inputs have keys: without them, the checkbox that appears with
    // the first todo would pair with the field, and the field the person
    // is typing in would turn into the checkbox.
    fn render(&self) -> View {
        let toggle_all = self.has_todos.then(|| {
            h("input")
                .key("toggle-all")
                .attr("aria-label", "Mark all as complete")
                .attr("class", "toggle-all")
                .attr("id", "toggle-all")
                .attr("type", "checkbox")
                .checked(self.all_completed)
                .on("change", "toggle-all")
        });
        let mut field = h("input")
            .key("new-todo")
            .attr("autofocus", "")
            .attr("class", "new-todo")
            .attr("placeholder", "What needs to be done?");
        if self.clear {
            field = field.value("");
        }
        h("div")
            .attr("class", "row")
            .child_if(toggle_all)
            .child(field.on("keydown", "keydown"))
            .into()
    }
}

element_kit::element!(TodoInput);
