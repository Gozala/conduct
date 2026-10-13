//! `<todo-item>`: one todo. It shows the checkbox, the title, and the
//! delete button, and edits the title in place after a double-click:
//! Enter or leaving the field saves the edit, and Escape cancels it. It
//! reports each change as a custom event that `<todo-app>` handles.

use element_kit::{Element, Event, Host, View, attribute, h};

struct TodoItem {
    host: Host,
    id: String,
    title: String,
    completed: bool,
    editing: bool,
}

impl TodoItem {
    fn save(&mut self, title: &str) {
        self.editing = false;
        self.host.emit("todo-edit", &format!("{}\n{title}", self.id));
    }
}

impl Element for TodoItem {
    const OBSERVED_ATTRIBUTES: &'static [&'static str] = &["todo-id", "title", "completed"];
    const STYLES: &'static str = include_str!("styles.css");

    fn new(host: Host, attributes: &[(String, String)]) -> Self {
        TodoItem {
            host,
            id: attribute(attributes, "todo-id").unwrap_or_default().into(),
            title: attribute(attributes, "title").unwrap_or_default().into(),
            completed: attribute(attributes, "completed") == Some("true"),
            editing: false,
        }
    }

    fn attribute_changed(&mut self, name: &str, value: Option<&str>) {
        match (name, value) {
            ("todo-id", Some(id)) => self.id = id.into(),
            ("title", Some(title)) => self.title = title.into(),
            ("completed", Some(value)) => self.completed = value == "true",
            _ => {}
        }
    }

    fn handle_event(&mut self, handler: &str, event: Event) {
        let value = event.value.as_deref().unwrap_or("");
        match handler {
            "toggle" => self.host.emit("todo-toggle", &self.id),
            "destroy" => self.host.emit("todo-destroy", &self.id),
            "edit" => self.editing = true,
            "keydown" => match event.key.as_deref() {
                Some("Escape") => self.editing = false,
                Some("Enter") if self.editing => self.save(value),
                _ => {}
            },
            "save" if self.editing => self.save(value),
            _ => {}
        }
    }

    // Attributes and events are listed by name, in the order conduct's
    // dictionaries keep them, so the node lists match conduct's exactly.
    fn render(&self) -> View {
        if self.editing {
            return h("li")
                .attr("class", "editing")
                .child(
                    h("input")
                        .key("edit")
                        .attr("aria-label", "Edit the todo")
                        .attr("autofocus", "")
                        .attr("class", "edit")
                        .value(self.title.as_str())
                        .on("blur", "save")
                        .on("keydown", "keydown"),
                )
                .into();
        }
        h("li")
            .attr("class", if self.completed { "completed" } else { "" })
            .child(
                h("input")
                    .key("toggle")
                    .attr("aria-label", "Toggle the todo")
                    .attr("class", "toggle")
                    .attr("type", "checkbox")
                    .checked(self.completed)
                    .on("change", "toggle"),
            )
            .child(h("label").child(self.title.as_str()).on("dblclick", "edit"))
            .child(
                h("button")
                    .attr("aria-label", "Delete the todo")
                    .attr("class", "destroy")
                    .child("×")
                    .on("click", "destroy"),
            )
            .into()
    }
}

element_kit::element!(TodoItem);
