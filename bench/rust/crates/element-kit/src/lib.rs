//! Bindings for the TodoMVC demo's element world, and the little an
//! element needs on top of them: a [`View`] tree to build, flattened into
//! the world's node list, and a [`Registry`] that keeps one Rust value for
//! each element of the tag, by the id `create` hands the host.
//!
//! An element that keeps its state in plain fields implements [`Element`]
//! and exports it with [`element!`]. One that does more, such as awaiting
//! requests in a handler, implements the generated `Guest` trait itself
//! and exports it with [`export!`], using the same views and registry.

wit_bindgen::generate!({
    path: "../../wit",
    world: "element-component",
    pub_export_macro: true,
    default_bindings_module: "element_kit",
    generate_all,
});

use std::collections::BTreeMap;

pub use exports::demo::element::element::{ElementNode, Event, Node, NodeKind, Property};

/// The host node of one element: the id the host knows it by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Host(pub u32);

impl Host {
    /// Dispatch the custom event `name` on the element's host node, with
    /// `detail`. It bubbles to the elements around this one.
    pub fn emit(&self, name: &str, detail: &str) {
        demo::element::host::emit(self.0, name, detail);
    }
}

/// One node of a view: an element with its children, or text.
#[derive(Clone, Debug)]
pub enum View {
    Element(Html),
    Text(String),
}

/// An element of a view, built with [`h`].
#[derive(Clone, Debug, Default)]
pub struct Html {
    tag: String,
    key: Option<String>,
    attributes: Vec<(String, String)>,
    properties: Vec<(String, Property)>,
    events: Vec<(String, String)>,
    children: Vec<View>,
}

/// A view of the element `tag`.
pub fn h(tag: impl Into<String>) -> Html {
    Html { tag: tag.into(), ..Html::default() }
}

impl Html {
    /// A stable identity among its siblings.
    pub fn key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
        self
    }

    /// Set the attribute `name`.
    pub fn attr(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.attributes.push((name.into(), value.into()));
        self
    }

    /// Set the DOM property `checked`.
    pub fn checked(mut self, checked: bool) -> Self {
        self.properties.push(("checked".into(), Property::Flag(checked)));
        self
    }

    /// Set the DOM property `value`.
    pub fn value(mut self, value: impl Into<String>) -> Self {
        self.properties.push(("value".into(), Property::Text(value.into())));
        self
    }

    /// Call the handler `handler` on the DOM event `event`.
    pub fn on(mut self, event: impl Into<String>, handler: impl Into<String>) -> Self {
        self.events.push((event.into(), handler.into()));
        self
    }

    /// Append `child`.
    pub fn child(mut self, child: impl Into<View>) -> Self {
        self.children.push(child.into());
        self
    }

    /// Append each of `children`.
    pub fn children<V: Into<View>>(mut self, children: impl IntoIterator<Item = V>) -> Self {
        self.children.extend(children.into_iter().map(Into::into));
        self
    }

    /// Append `child`, if there is one.
    pub fn child_if(mut self, child: Option<impl Into<View>>) -> Self {
        self.children.extend(child.map(Into::into));
        self
    }
}

impl From<Html> for View {
    fn from(html: Html) -> View {
        View::Element(html)
    }
}

impl From<&str> for View {
    fn from(text: &str) -> View {
        View::Text(text.into())
    }
}

impl From<String> for View {
    fn from(text: String) -> View {
        View::Text(text)
    }
}

impl View {
    /// The view as the world's node list: the tree in document order,
    /// each node naming its parent by its index.
    pub fn into_nodes(self) -> Vec<Node> {
        let mut nodes = Vec::new();
        self.flatten(None, &mut nodes);
        nodes
    }

    fn flatten(self, parent: Option<u32>, nodes: &mut Vec<Node>) {
        match self {
            View::Text(text) => nodes.push(Node { parent, key: None, kind: NodeKind::Text(text) }),
            View::Element(Html { tag, key, attributes, properties, events, children }) => {
                let index = nodes.len() as u32;
                nodes.push(Node {
                    parent,
                    key,
                    kind: NodeKind::Element(ElementNode { tag, attributes, properties, events }),
                });
                for child in children {
                    child.flatten(Some(index), nodes);
                }
            }
        }
    }
}

/// The value of the attribute `name` among `attributes`, if it has one.
pub fn attribute<'a>(attributes: &'a [(String, String)], name: &str) -> Option<&'a str> {
    attributes.iter().find(|(n, _)| n == name).map(|(_, v)| v.as_str())
}

/// An element whose state lives in its own fields. The host creates one
/// for each element of the tag, tells it what happens to that element,
/// and renders it after each change.
pub trait Element: Sized + 'static {
    /// The attributes whose changes the element observes.
    const OBSERVED_ATTRIBUTES: &'static [&'static str];
    /// The CSS of the element's shadow root.
    const STYLES: &'static str;

    /// A new element on `host`, with its first attributes.
    fn new(host: Host, attributes: &[(String, String)]) -> Self;

    /// The element has connected to the document.
    fn connected(&mut self) {}

    /// The element has left the document.
    fn disconnected(&mut self) {}

    /// The observed attribute `name` changed to `value`, or was removed.
    fn attribute_changed(&mut self, _name: &str, _value: Option<&str>) {}

    /// The DOM event `event`, which a view sent to `handler`, happened.
    fn handle_event(&mut self, _handler: &str, _event: Event) {}

    /// The element's contents.
    fn render(&self) -> View;
}

/// The live elements of one tag, by the id the host knows each by.
pub struct Registry<E> {
    next: u32,
    elements: BTreeMap<u32, E>,
}

impl<E> Default for Registry<E> {
    fn default() -> Self {
        Registry { next: 1, elements: BTreeMap::new() }
    }
}

impl<E> Registry<E> {
    /// Keep the element `make` builds for a new id, and return the id.
    pub fn insert(&mut self, make: impl FnOnce(Host) -> E) -> u32 {
        let id = self.next;
        self.next += 1;
        self.elements.insert(id, make(Host(id)));
        id
    }

    /// Forget the element `id`, and answer it.
    pub fn remove(&mut self, id: u32) -> Option<E> {
        self.elements.remove(&id)
    }

    /// The element `id`. The host only names elements it created, so an
    /// unknown id is a broken host, and traps.
    pub fn get(&self, id: u32) -> &E {
        self.elements.get(&id).unwrap_or_else(|| panic!("no element has the id {id}"))
    }

    /// The element `id`, to change.
    pub fn get_mut(&mut self, id: u32) -> &mut E {
        self.elements.get_mut(&id).unwrap_or_else(|| panic!("no element has the id {id}"))
    }
}

/// Export the [`Element`] `$ty` as the component's `demo:element/element`.
#[macro_export]
macro_rules! element {
    ($ty:ty) => {
        struct Component;

        ::std::thread_local! {
            static ELEMENTS: ::std::cell::RefCell<$crate::Registry<$ty>> =
                ::std::cell::RefCell::new($crate::Registry::default());
        }

        impl $crate::exports::demo::element::element::Guest for Component {
            async fn observed_attributes() -> ::std::vec::Vec<::std::string::String> {
                <$ty as $crate::Element>::OBSERVED_ATTRIBUTES.iter().map(|name| (*name).into()).collect()
            }

            async fn styles() -> ::std::string::String {
                <$ty as $crate::Element>::STYLES.into()
            }

            async fn create(attributes: ::std::vec::Vec<(::std::string::String, ::std::string::String)>) -> u32 {
                ELEMENTS.with_borrow_mut(|elements| {
                    elements.insert(|host| <$ty as $crate::Element>::new(host, &attributes))
                })
            }

            async fn connected(id: u32) {
                ELEMENTS.with_borrow_mut(|elements| $crate::Element::connected(elements.get_mut(id)))
            }

            async fn disconnected(id: u32) {
                if let Some(mut element) = ELEMENTS.with_borrow_mut(|elements| elements.remove(id)) {
                    $crate::Element::disconnected(&mut element);
                }
            }

            async fn attribute_changed(id: u32, name: ::std::string::String, value: Option<::std::string::String>) {
                ELEMENTS.with_borrow_mut(|elements| {
                    $crate::Element::attribute_changed(elements.get_mut(id), &name, value.as_deref())
                })
            }

            async fn handle_event(id: u32, handler: ::std::string::String, event: $crate::Event) {
                ELEMENTS.with_borrow_mut(|elements| {
                    $crate::Element::handle_event(elements.get_mut(id), &handler, event)
                })
            }

            async fn render(id: u32) -> ::std::vec::Vec<$crate::Node> {
                ELEMENTS.with_borrow(|elements| $crate::Element::render(elements.get(id))).into_nodes()
            }
        }

        $crate::export!(Component with_types_in $crate);
    };
}
