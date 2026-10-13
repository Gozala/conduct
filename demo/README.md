# conduct TodoMVC Demo

The [Wasm Component Model Polyfill]'s TodoMVC demo, with conduct in place of
Zena. Its elements and routes are the conduct programs in
[`../examples`](../examples). The page and the service worker each run the
conduct compiler as a component on the polyfill
([`../compiler`](../compiler)), and compile the sources when the demo starts.
A person can edit any source in the shelf and see the change at once.

It is published at <https://gozala.github.io/conduct/>.

The host framework is the polyfill demo's (`rust/wcmp-demo`, MPL-2.0), with
what was Zena's replaced:

- `src/compiler/` runs the conduct compiler component. It imports nothing, and
  each call hands it the whole source, so there is no source bundle and no
  `read-source` import.
- `src/glue/` is only a compile's request: conduct writes the exports of the
  element world and the route world itself, so the host writes no glue.
- `src/highlight/` colors conduct's tokens: definitions, labels, keywords
  (variants), type variables, and tasks (names that end in `!`).
- The WASI interfaces Zena's output imports (`wasi:cli`'s standard output
  and error, the monotonic clock) are gone: conduct's components import none.
- `build.sh` builds the site into `dist/` with `wasm-bindgen` in place of
  Trunk. Serve `dist/` over HTTP from any path.
- `tests/browser.py` drives the built site in Chromium with Playwright:
  TodoMVC's flows, and the shelf's hover, problems, and save.

## Tracing

The page and the service worker each record the spans of the polyfill, the
runtime layer, and the demo with `tracing-web`. Each span becomes user timing
marks and a measure, named after the span. Record a profile in the browser's
performance panel and open the timings track to see them: the conduct compiles,
the element renders and their diffs, the routes, the todo model, IndexedDB, and
under them the polyfill's compiles, instantiations, and calls. Events at `INFO`
and above go to the console.

The spans are at `DEBUG` and above by default. The `trace` parameter of the
page's URL names another level: `?trace=trace` adds the polyfill's turns, its
canonical ABI, and its host calls, which are many, and `?trace=off` records
none. The page hands the parameter to the service worker. A service worker
keeps its own timeline: to see its spans, profile the worker itself, in the
DevTools that `chrome://inspect/#service-workers` opens for it. Each context
clears its timeline every five seconds, so that the marks do not pile up while
nothing records them.

## Editor

The shelf's editor is a text area over two layers: one colors conduct's tokens,
and one underlines problems. `src/assist.rs` asks the compiler component's
`check` for the problem in a source, or for the type of each definition, and
answers the rest from that check.

- A pause in typing checks the source, underlines its problem, and lists it.
- Resting the pointer on the name of a definition shows its inferred type.
- Ctrl-Space shows the source's names and conduct's forms; Enter or Tab
  inserts one.
- F12 or a Ctrl-click (Cmd-click) goes to a definition in the same source.

conduct has no formatter yet, so the shelf has no Format action.

## Fixed Here, Still in the Zena Version

- `<todo-input>` renders the "mark all" checkbox before the field only once
  there are todos, with no keys, so the first add pairs the checkbox with
  the field: the field the person is typing in becomes the checkbox, and
  loses focus. conduct's version keys both.
- The editor measures a character with the text area's computed `font`,
  which Chrome reports empty when `font-variant-ligatures` is set, so hover
  and Ctrl-click land on the wrong column. `src/editor.rs` builds the font
  from its longhands.

[Wasm Component Model Polyfill]: https://github.com/dialog-db/wasm-component-model-polyfill
