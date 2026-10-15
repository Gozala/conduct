# Open questions

Decisions taken without waiting, each with what was chosen, why, and what
would reverse it. Newest first. Go over these together; anything marked
**revisit** is a choice that gets more expensive to change the longer it
stands.

## A name of the source shadows a built-in, silently

- **Found** writing conduct.codec's bytes: the library defines a codec
  `byte`, so inside it `(byte n or: 0)` called the codec, not the
  built-in, and the checker's error ("a record is not a function of 2
  arguments", in another definition of the same cycle) did not say why.
  `conduct/byte` fixes it. Same-source-first is what makes `codec/map`
  possible, and this is its cost.
- **Possible fix:** when a source's own name hides a built-in of the
  same name, try the built-in too when the call does not check, as type
  resolution does for names that are ambiguous; or warn. Your call,
  since it makes resolution less predictable to read.

## The bytes format of conduct.codec

- **Chosen:** elm-serialize's idea (compact, positional), not its
  encoding (it writes every number as a float64, for JavaScript).
  LEB128 and zigzag as protobuf does.
- **Open:** no version byte (elm-serialize prepends one), and no way to
  evolve a variant but appending rows. If codec bytes go into Dialog,
  this wants a decision first.

## Small literals are bytes

- **Chosen:** a literal below 256 has type `byte`, the narrowest view
  that holds it, so `(define x 5)` shows `byte`. Precise, and needed for
  `[0xff 0xd8]` to be bytes, but it reads oddly where nothing is binary.
- **Revisit** if it bothers you in practice: the alternative is literals
  as `natural` and a conversion, `(byte 255 or: 0)`, wherever bytes are
  written by hand. Dialog has no byte type that I know of; if its
  numbers carry a precision (`natural:8`?), the name should follow it.

## conduct.codec's names are elm-codec's

- **Chosen:** `codec/text`, `codec/list`, `codec/map` rather than
  `text-codec`; the bare `map` in user code is resolved by type between
  the built-in and the codec's. This is the reading-versus-writing
  question from names: if you want text to always say which `map`, the
  library should take suffixed names instead.
- **Variants:** elm-codec's `{"tag", "args"}` shape, for interop, rather
  than `{"circle": 2}` (serde's default). Open if Dialog wants another.
- **Next:** the same codecs over bytes, once bytes land, as you asked;
  elm-serialize's one codec for both formats is the model to compare
  against.

## A claim's hash names its types, not what they are

- **Found:** `(claim (size jv) : natural)` hashes the same whichever
  `jv` is declared, because a claim is hashed as written. Two codebases
  that declare `jv` differently give one hash to two different
  signatures. Harmless while a codebase is one set of sources; wrong
  once definitions are shared.
- **Planned:** with types hashed (Unison's unique and structural types,
  below in "invariants"), a claim hashes the types it names by their
  hashes.

## Private names, invariants and tests (discussed Oct 14)

- **Found:** hiding functions does not protect invariants in conduct:
  anyone who can name an attribute can build or read a record with it.
  What would: private attributes (only their domain constructs, reads
  or matches them), opaque types that structural subtyping cannot see
  into, no reflective record operations that bypass them, and a codec
  defined in the type's domain as the only way in from outside, since a
  value from storage or a peer was never built by the constructor.
- **Held** at your call. Open: whether an opaque type's identity is by
  name (Unison's unique types) so stored values survive a library
  change.
- **Tests:** Racket's `(module+ test ..)` gives tests beside the code,
  access to private names, and no cost to users. Here a `(test ..)` form
  in a domain would need no module system: nothing reaches a test, so
  it is never compiled into a program, and a pure test's result can be
  cached by its hash, as Unison does. Not built.

## Names resolved by type cost a check per candidate

- **Chosen:** where a name reaches several definitions, each is tried by
  checking the whole component, recursively for further ambiguous
  names. Exact names and same-source definitions are tried first, so
  this only runs for a genuinely ambiguous name.
- **Revisit** if sources with many ambiguous names get slow: the
  attempts multiply. Checking per name, in the checker, rather than per
  component, is the fix.
- **Open, for you:** whether a name settled by type should be written
  back with its prefix when code is printed, so that what the text says
  never depends on what else is in the codebase.

## A module carries only the runtime it reaches

- **Done:** every module used to carry the whole runtime, which the
  text, bytes, order and JSON primitives had grown to the point that
  todo-item was 37 KB where the benchmark measured 29 KB. Now a module
  keeps the runtime functions it reaches: todo-item is 26.7 KB,
  todo-input 23.6 KB, todo-footer 23.0 KB, todo-app 54.3 KB, the
  routes 28 KB. The benchmark's other numbers were not rerun.

## Integer arithmetic and reference counts are inline

- **Done:** `+`, `-`, `*` and `=` are compiled in place, a built-in
  borrows a local that outlives the call instead of counting it up and
  down, numbers and constants are never dropped, and every `dup` and
  `drop` of a local tests for an object before calling. A counting loop
  went from 51 ns to about 1 ns an iteration, and summing a list of two
  million from 0.48 s to 0.20 s (`tests/speed.rs`, ignored by default).
- **Cost:** 6% larger components, before the runtime was shaken.

## Unclaimed combinator code can exhaust the type checker

- **Found:** without claims, the JSON library ran the checker out of
  memory. Libraries now claim their types, which fixes it for them.
- **Revisit:** user code written in the same style (many small
  higher-order functions over a record) would hit it too; the general
  fixes are structure sharing between types and simplifying bounds as
  they are added. Worth doing before more people write conduct.

## JSON shape

- **Chosen:** JSON values are tagged variants, objects are ordered lists
  of members, non-integers are `(decimal text)`.
- **Revisit** when conduct has decimals, and if objects should be
  `conduct.map` maps for lookup speed.

## The router design is new, not the one you approved

- **Found:** you said an earlier router proposal "sounds great", but it
  was lost when this conversation's context was compacted, and it was
  not written down in the repo. `conduct.route` is a fresh design: a
  `route` macro over parser pipelines, parameters as records of declared
  attributes. Check it against what you remember agreeing to.

## Route segments

- **Chosen:** a text segment is the bytes up to `/`, `?` or `#`, with
  `%XX` decoded and the bytes read as UTF-8; formatting escapes every
  byte that is not RFC 3986's `pchar`, so text with `/` or a space in it
  round-trips. Routes match paths only (no method, no query).
- **Why no method or query yet:** they belong with the route world's
  request record, which the route world does not yet build from a route
  value.

## Macro labels shadow literal labels

- **Chosen:** a template label named like a pattern variable is
  substituted. A macro that wanted the literal label must rename its
  variable. Racket's syntax-case has the same behavior for identifiers.

## Parser attribute renamed `problem` to `reason`

- **Why:** the route world's prelude defines a function `problem`, and
  attribute names are codebase-wide (see below), so the two could not be
  loaded together.

## PR descriptions carry a Claude footer

- **Found:** this session's GitHub access appends "Generated by Claude
  Code" to every pull request description it writes, and re-adds it on
  edit, so I cannot remove it from here. Commits are clean. The
  descriptions of conduct #27 to #38 and perma-map #14 have it; strip
  them by hand if they matter.

## Parser API names

- **Chosen:** elm/parser's names where they do not collide with a
  built-in (`succeed`, `one-of`, `and-then`, `token`, `symbol`,
  `keyword`, `chomp-while`, `sequence`, `loop`), and new ones where they
  would: `map-value` (not `map`), `fail` (not `problem`), `skip` and
  `keep` for `|.`, and `parsing` for pipelines. A library's bare name
  that a built-in also has makes every bare use ambiguous.
- **Revisit** now that names resolve by type: inside the library its own
  `map` would come first, and a user's `map` would be chosen by type
  between `conduct/map` and the parser's. Elm's names would work; the
  cost is that a reader can no longer tell which `map` from the text.

## Numbers in parsers

- **Chosen:** `int` parses decimal naturals; `0x`, `0o`, `0b` and
  anything float-like fail as invalid, as elm/parser's `int` does.
  Digits past 62 bits wrap, as conduct's arithmetic does.
- **Revisit** with wider numbers and floats (Dialog's are 128-bit and
  decimal).

## Tail calls through closures

- **Chosen:** only direct calls in tail position are tail calls.
- **Why:** a closure call drops the closure after the callee returns,
  because the callee only borrows its captures. Making it a tail call
  means the callee takes the closure and drops it, a change to the
  calling convention of every function.
- **Revisit** if combinator code (parsers, codecs) hits stack limits on
  real inputs.

## Attributes are named codebase-wide

- **Found:** a local attribute name such as `value` belongs to one
  attribute across the whole codebase (`conduct.event/value`), whatever
  the domain. Libraries pick distinct names (`entry-key`, `parsed`).
- **Planned:** attributes named in domains, with identity as name plus
  type, which is already on the list. Until then, library attribute
  names are a shared namespace.

## No zero-parameter functions, so no `lazy`

- **Chosen:** recursive parsers are written as functions of the parse
  state, `(define (expr s) ((one-of [...]) s))`, rather than adding
  thunks. A definition with no parameters is a value by design.

## Text is UTF-8 bytes, and positions in it are byte offsets

- **Chosen:** text primitives take and give byte offsets
  (`(slice t from: 0 to: 3)`, `(code-point-at t at: i or: d)`), as Rust
  and Roc index `Str`, not Elm's UTF-16 units or code point counts.
- **Why:** a parser moves through its source by offsets, and a byte
  offset is O(1) to follow; the same parser code can then run over bytes
  later, which the codec and router want. A code point count would make
  every step a scan.
- **Cost:** `(slice t from: 1 to: 2)` can cut a character in half. The
  primitives decode a code point where one starts and answer the default
  elsewhere, and a parser only slices at offsets it reached by stepping
  over whole characters.
- **Revisit** if text should be a sequence of code points or grapheme
  clusters in the language's eyes (Swift's choice).

## Function equality is Unison's

- **Chosen** (you, Oct 14): a function is its code's hash and what it
  captured, in every module.
- **Open:** a lambda and a top-level definition with the same body have
  different ids, because a definition's hash includes its claim. Unison
  would call them one term. Harmless until a stored value holds a
  function.

## `(natural x)` returning a variant

- **Chosen:** not added. `(natural x or: d)` is the only conversion.
- **Why:** nothing written in conduct so far needs to tell a negative
  result apart from a default. The parser and codec ports will show
  whether it is wanted; adding it is purely additive.

## Hash strength

- **Chosen:** keep 32-bit FNV-1a with no key for now.
- **Why:** rehashing (`(hash [frame key])`) already stops colliding keys
  from degrading a map into scans; what a weak hash still allows is
  making tries deeper by brute force. That matters once maps hold keys
  from untrusted peers, which waits on values stored in Dialog.
- **Revisit** before conduct values are synced: a keyed hash (SipHash,
  or BLAKE3 truncated, which the compiler already uses) with a key per
  replica.

## Map keys may be any value

- **Chosen** (follows from Unison equality): functions, views and every
  other value may be keys; `hash` reads all that `=` compares.
- **Open:** whether keys that Dialog cannot store (functions) should be
  allowed in maps that are meant to be persisted. That is a question for
  when maps are persisted.

## perma-map PR #14

- Nine fixes plus a CI update, open for your review. UnixFS trees are
  unchanged except where they had lost an entry (differential test).
- Not checked: whether js-ipfs and go-unixfs produce the same trees after
  deletes.
