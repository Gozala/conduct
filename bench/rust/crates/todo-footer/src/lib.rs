//! `<todo-footer>`: how many todos are left, the three filters, and the
//! button that clears the completed todos, which shows only when there
//! are some. A filter is a link to a URL hash, which the page turns into
//! `<todo-app>`'s `filter`.

use element_kit::{Element, Event, Host, Html, View, attribute, h};

struct TodoFooter {
    host: Host,
    active: String,
    completed: String,
    filter: String,
}

impl TodoFooter {
    fn link(&self, filter: &str, label: &str, href: &str) -> Html {
        let class = if self.filter == filter { "selected" } else { "" };
        h("li").child(h("a").attr("class", class).attr("href", href).child(label))
    }
}

impl Element for TodoFooter {
    const OBSERVED_ATTRIBUTES: &'static [&'static str] = &["active", "completed", "filter"];
    const STYLES: &'static str = include_str!("styles.css");

    fn new(host: Host, attributes: &[(String, String)]) -> Self {
        let get = |name| attribute(attributes, name).unwrap_or_default().to_owned();
        TodoFooter { host, active: get("active"), completed: get("completed"), filter: get("filter") }
    }

    fn attribute_changed(&mut self, name: &str, value: Option<&str>) {
        let Some(value) = value else { return };
        match name {
            "active" => self.active = value.into(),
            "completed" => self.completed = value.into(),
            "filter" => self.filter = value.into(),
            _ => {}
        }
    }

    fn handle_event(&mut self, handler: &str, _event: Event) {
        if handler == "clear" {
            self.host.emit("todo-clear", "");
        }
    }

    fn render(&self) -> View {
        let left = if self.active == "1" { " item left" } else { " items left" };
        let clear = (self.completed != "0" && !self.completed.is_empty()).then(|| {
            h("button").attr("class", "clear-completed").child("Clear completed").on("click", "clear")
        });
        h("footer")
            .attr("class", "footer")
            .child(h("span").attr("class", "todo-count").child(h("strong").child(self.active.as_str())).child(left))
            .child(h("ul").attr("class", "filters").children([
                self.link("all", "All", "#/"),
                self.link("active", "Active", "#/active"),
                self.link("completed", "Completed", "#/completed"),
            ]))
            .child_if(clear)
            .into()
    }
}

element_kit::element!(TodoFooter);
