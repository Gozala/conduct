# Decisions

Choices made while building, each with the reason and what would reverse
it. Newest first.

## Routes from one description, in both directions

- **`conduct.route` follows subroute:** a route parses a path into
  typed parameters and formats parameters back into the path, so a link
  is made from the route it points at. Subroute builds both from a
  tagged template at run time; here `route` is a macro that walks the
  description once at compile time and writes a `parsing` pipeline for
  one direction and a `str` of the same pieces for the other.
- **Parameters are a record of declared attributes,** `(a
  natural-segment)` capturing the attribute `a`. Subroute builds an
  object with dynamic keys and merges objects; conduct has neither, and
  a record of declared attributes gives each parameter its attribute's
  type and identity.
- **Macros write labels:** a template's `name:` is the label of what the
  pattern variable `name` matched, and `...` after a label and a form
  repeats the pair. That is what lets `route` build `{a: a, b: b}`.
  A template that wants a literal label with the same name as one of
  its pattern variables can no longer have it; renaming the variable
  is the way out.
- **Not yet:** methods and query parameters, percent-encoding of
  segments (a text segment is raw text up to `/`, `?` or `#`), a capture
  that runs to the next literal rather than the next `/`, and the route
  world deriving its `pattern` from a route value.

## A parser library, ported from elm/parser

- **`conduct.parser` is elm/parser's `Parser.Advanced`:** a parser is a
  function of the parse state answering `(ok {progress parsed after})` or
  `(error {progress dead-ends})`, and `progress` decides whether
  `one-of` tries its next branch. Problems and contexts are any values.
  elm/parser is BSD 3-Clause; its notice is in `lib/LICENSE-elm-parser`.
- **Pipelines bind names** (`parsing`, a macro over `and-then`) where
  Elm has `|=` and `|.`: conduct has no infix operators, and a function
  called through a value must be called with its labels, so `map2` with
  a two-argument callback would make every caller name its parameters
  as the library does. `skip` and `keep` remain for the unnamed case.
- **No `lazy`:** a definition with no parameters is a value, so a
  recursive parser is a function of the parse state.
- **One deliberate difference:** `chomp-until` consumes the text it
  looks for, with a row and column that agree with its offset.
  elm/parser's kernel leaves the offset at the start of the match but
  counts rows and columns to its end, so a parser after it sees a
  position that does not match its offset.
- **Not ported yet:** `number` (hexadecimal, octal, binary, floats;
  conduct has no floats), `mapChompedString`, nestable comments, and the
  simple `Parser` module's fixed problem type, beyond `symbol`'s
  `(expecting text)`.
- **Tested** on what it parses and where it fails, backtracking, names
  with reserved words, every trailing-separator rule, recursion,
  positions across lines and comments, context stacks, and a list of
  20,000 items, which needs the tail calls of the previous entry.

## Calls in tail position are tail calls

- **A loop was a stack of frames.** Recursion is conduct's only loop,
  and every call was a Wasm `call`, so a tail-recursive count of 10,000
  exhausted the stack. A parser over a long input, or any walk over a
  long list, would trap.
- **A direct call in tail position is now `return_call`.** Tail position
  passes from a function's body through `if` arms, a `let`'s body,
  `match` arms, `cond`, and the second operand of `and`/`or`. Perceus
  leaves nothing to do after such a call: a local is dropped as soon as
  nothing later uses it, and the call's arguments moved the rest. A test
  runs a million calls deep, through a cycle too.
- **Not yet: calls through a closure.** `$applyN_take` drops the
  closure after the call, since the callee borrows what it captured, so
  a closure call cannot simply be `return_call_indirect`; the callee
  would have to take the closure. Parser combinators make such calls, so
  a loop written through closures still grows the stack; `loop` in the
  parser library recurses directly.
- **Tail calls are Wasm 3.0,** on in Wasmtime by default and in current
  browsers (Chrome 112, Firefox 121, Safari 18.2).

## Text by byte offset

- **Text is UTF-8, and positions in it are byte offsets,** as Rust and
  Roc index strings: `(slice t from: a to: b)`, `(byte-at t at: i or:
  d)`, `(code-point-at t at: i or: d)`, `(occurs? small in: t at: i)`,
  `(index-of small in: t from: i or: d)`, `(from-code-point c)`. A parser
  walks its source by offsets, and a byte offset is followed in one
  step; a count of code points would make each step a scan. elm/parser's
  kernel (`isSubString`, `isSubChar`, `findSubString`) is these.
- **What can be missing takes a default,** as `item` does: a code point
  is missing past the end, in the middle of one, or where the bytes are
  not UTF-8. The default may be any value, so `(code-point-at t at: i
  or: :end)` answers `[natural :end]`.
- **A slice clamps** to the text and is never negative, and may cut a
  character in half; a parser slices only at offsets it reached by
  stepping over whole characters. `(from-code-point c)` writes U+FFFD
  for a surrogate or a number past U+10FFFF.
- **Offsets are naturals,** so the checker refuses `(byte-at t at: (- i
  1) or: d)` until the caller says what a negative offset means.

## Keys are rehashed, not bucketed

- **A key's digest never runs out.** conduct.map reads 32 bits at a
  time: frame 0 is `(hash key)`, and after six levels of five bits the
  next level reads `(hash [1 key])`: the key with the frame number
  added, hashed again. Depth stays the trie's concern, which frame a
  level reads; `hash` stays one function of one value. Keys that collide
  in one frame part in a later one,
  so there are no collision buckets. This is perma-map's UnixFS
  configuration, which hashes the key with the frame number appended,
  and Bagwell's original suggestion for a hash that is used up.
- **Why over sorted buckets:** a key's place is a function of the key
  alone, so the trie is canonical with no order over keys; there is no
  bucket code (seven of perma-map's nine bugs were in it); and keys
  chosen to collide cost an attacker one more frame per level instead of
  turning lookups into scans of a bucket, which matters for keys synced
  from peers.
- **What it needs: `hash` reads everything `=` compares.** Otherwise two
  keys that are not equal hash alike in every frame, and the trie
  descends forever looking for a bit that tells them apart. `hash` read
  only the kind and length of functions, views, emits and fetches;
  functions that differed in what they captured hashed alike. It now
  reads a function's code and captured values and the values of the
  others, and a test keeps functions as keys.
- **A function is its code's hash and what it captured,** as Unison
  identifies terms. `=` compared, and `hash` would have read, a
  closure's place in its module's function table, which another module
  numbers differently. Each module now carries a code id per table
  entry: a definition's hash, or for a lambda the hash of its term with
  captured locals renumbered by slot, so the same lambda written in two
  definitions is one function. Tests compile one closure into two
  modules that place it differently and get one hash.
- **What it does not fix:** FNV-1a over 32 bits with no key is weak, and
  frames of it are related. A seeded, stronger hash is the next step
  before maps hold keys from untrusted peers in earnest.
- **`compare` stays,** for sorted maps and sets, but conduct.map no
  longer uses it.

## Arithmetic is precise

- **`+`, `*` and `quot` keep what they are given:** each argument flows
  into one variable bounded by `integer`, which is the result, so
  naturals in give a natural out and an integer anywhere gives an
  integer. `rem` has the dividend's type (Wasm's `rem_s` takes the
  dividend's sign); `-` answers an integer.
- **Precision over shorter types.** The cost is that a function of a
  number shows its bound and the union it answers, `((\ (& ?a integer)) :
  [?a natural])` where it was `((\ integer) : integer)`. That type says
  something true the short one could not: `fib` is now `((\ integer) :
  natural)`, and conduct.map's `deeper` no longer converts its sum back
  to a natural. Errors follow the same flow: a sum claimed as `text`
  now reports that a natural is not a text, from its literal.
- **What would shorten them:** printing a variable bounded by `integer`
  that a union joins with `natural` as a named number type, or showing
  a definition's type at the use site, where it is concrete. Neither is
  done; both are display, not checking.

## Equal maps are the same value

- **The question:** a hash trie's shape is fixed by its entries except
  in collision buckets, which hold keys whose whole hashes are equal in
  the order they came. Two maps of the same entries could then differ in
  structure, so structural `=` and `hash` would call them different.
- **Clojure does not make the structure canonical; it makes equality a
  method.** `=` on a map (`APersistentMap.equiv`, ClojureScript's
  `equiv-map`) checks the other is a map of the same count, then looks
  up each key; the hash is an order-independent sum over entries
  (`Murmur3.hashUnordered`, `hash-unordered-coll`). Collision buckets
  keep insertion order, visible to `seq` and never to `=`. Clojure's
  tries do not compact on delete either.
- **CHAMP makes the structure canonical so equality can be
  structural:** compact on delete, so equal maps have equal bitmaps and
  shared subtrees short-circuit; only collision buckets, whose layout
  "depends on dynamic insertion and deletion order", are compared as
  sets (Steindorfer and Vinju, OOPSLA 2015, section 4.3; Clojure's way is
  its footnote's alternative).
- **conduct went one step past CHAMP: buckets were sorted too,** by
  `compare`, as perma-map sorts them. ("Keys are rehashed, not
  bucketed" then removed the buckets.) Every part of the trie is then a
  function of its entries, so the one structural `=` and `hash` are
  right for maps with no dispatch, and a content hash of a map (a merkle
  reference, once values are stored in Dialog) agrees with `=`. The
  per-type dispatch Clojure relies on would let `=` and a value's
  identity disagree, which a hash-addressed language cannot afford.
  `=` already returns at once for the same object, CHAMP's fast path for
  a map derived from another.
- **`compare` is a total order over every value that agrees with `=`:**
  numbers, Booleans, text, keywords, lists, records and dictionaries,
  functions, views, emits, fetches, in that order between sorts, and
  within one by value, by bytes, or by contents in order with a prefix
  first. Records order by their field ids, which hash the attribute
  names, so the order between records is fixed but not alphabetical.
  It also gives sorted maps and sets something to sort by.
- **Two runtime bugs it found.** A record's field ids are raw words, and
  `=` and `hash` walked them as values: an id that is a multiple of 8,
  one name in eight (`conduct.map/root` is one), was read as a pointer,
  which trapped or read stray memory. And a dictionary's keys are text
  in a literal but keywords once `assoc` puts them; `=` matched them by
  bytes, but `hash` did not, so equal dictionaries hashed differently.
  Field ids are now compared as words, and keys hash and order by their
  bytes.

## A hash map, written in conduct

- **The first library is written in the language, not built in.**
  `conduct.map` (`lib/map.cdt`) is a port of perma-map: a hash array
  mapped trie after CHAMP, five bits of a 32-bit `hash` a level, two
  bitmaps a node, and a list of entries where the hash is used up after
  seven levels. It needed only what the runtime already had for it:
  bits, `hash`, lists updated by index, and `natural`, a total way from
  an integer to a natural (`(natural (- bit 1) or: 0)`).
- **Nodes are canonical,** as CHAMP's are: removing pulls a child left
  with one entry back into its parent, at every level. So maps of the
  same entries are `=` whatever order they were made in (see "Equal maps
  are the same value" for keys whose whole hashes collide).
- **No transient builders and no configurable hash.** perma-map has
  both; here a map nothing else holds is updated in place, and there is
  one `hash`, over a value's structure.
- **Size costs a second walk.** `put` and `remove` ask `contains?`
  first. Having `place` answer whether it added an entry would save it;
  that needs a pair of results, which is a record, and a record is
  allocated. Not measured yet.
- **Claimed types are not yet shown by their names.** The library
  declares `(map ?k value: ?v)` and claims every definition with it, so
  the checker holds the code to those types, but what it shows is the
  type expanded: `{map-root: (rec ?a {..}), map-size: integer}`. Showing
  a type by the name it was declared under is the next thing an editor
  needs from the checker.
- **Tested against a model:** 1500 random puts and removes compared with
  Rust's `BTreeMap`, removal down to `empty`, `=` across orders, keys
  whose whole hashes collide, and every run checked for leaks. FNV-1a
  spreads small integers well: no two integers below 2^25 hash alike,
  so the colliding pairs are written into the test.
- **Two bugs in perma-map's 32-bit path, found while porting.** Its
  tests use only the 8-bit `Uint8Array` configuration, so neither shows
  there. `Uint32` `unset` masks with `0xff ^ (1 << index)`, which clears
  every bit from 8 up, so a node loses its entries in slots 8 to 31 the
  first time any bit is unset, which an insert does too when two keys
  meet in a slot. And `popcount(bits)` with no index counts the bits
  below 31, so slot 31 never counts toward a node's arity: a child
  holding slots 29 and 31 reads as one entry, and the parent inlines it
  and drops the other. With both fixed, the 32-bit path agrees with a
  model over 40,000 operations. Separately, its default hasher is
  `murmur.x64.hash126`, which does not exist, so the default
  configuration cannot hash a key without a hasher passed in.

## Names are pointers, in domains

- **Every definition is its hash, and a name is only a pointer to one.**
  There are no modules and no imports. A name is resolved once, when its
  source is added, and the hash is what is stored; a definition added
  later never changes what an earlier one meant. That is what makes a
  bare name that guesses safe, as Unison's suffix names are.
- **A domain is Dialog's:** `(domain io.gozala.parser)` first in a
  source puts its definitions there, as an attribute's domain does in
  Dialog, so one naming scheme covers what the database stores and what
  the codebase defines. Dialog's grammar applies to the domain.
- **Three spellings:** whole, `io.gozala.parser/succeed`; relative,
  `./succeed`, in the source's own domain; and bare, `succeed`. A bare
  name is a definition with no domain if there is one, else the one
  definition of that name in any domain. Several domains with it, or one
  and a built-in, make it ambiguous, and the error lists each whole name.
  A definition of the same source comes first, so a recursive call needs
  no `./`. A built-in's whole name is `conduct/count`.
- **Not yet:** a name that points at several hashes, which an editor
  lets a person choose among, and a pinned `name#hash` for text to say
  which; the same full name added again moves the name to the new hash.
  Attributes, types and macros are not named in domains yet: an
  attribute's identity becomes its name and its type, hashed as Dialog
  hashes it, next.

## Naturals, and what a standard library builds on them

- **`natural` is a subtype of `integer`,** as Dialog's `natural:` is a
  view of its integers. A literal that is not negative is a natural, and
  flows wherever an integer does; a union of the two is an integer, and
  an intersection a natural.
- **Arithmetic stayed on integers at first,** for shorter types;
  "Arithmetic is precise" reverses it. `count`, the bit operations and
  `hash` answer naturals.
- **Numbers hold 62 bits for now.** Dialog's are 128-bit; widening needs
  a boxed form for what does not fit in a tagged `i64`. Arithmetic does
  not yet check for overflow; `shift-left` drops the bits past 62.
- **Bits and hashes are built in:** `bit-and`, `bit-or`, `bit-xor`,
  `shift-left`, `shift-right`, `count-ones`, and `hash`, a 32-bit FNV-1a
  over a value's structure, so equal values hash alike. They are what a
  hash array mapped trie needs, which is the first library to port.
- **Lists are updated by index:** `(item xs at: i or: d)`, `set-at`,
  `insert-at`, `remove-at`. An index past the end is not an error: `item`
  answers the default, `set-at` and `remove-at` answer the list as it
  is, and `insert-at` appends. Each consumes its list and changes it in
  place when nothing else holds it, as `conj` does.
- **Hexadecimal and binary literals,** `0x1f` and `0b1010`.

## What is shown can be claimed

- **Every type the checker shows is a claim that checks,** and shown
  again is the same type. A test claims every definition of the
  examples, and a set of tricky ones, with the type shown for it.
- **Claims gained what printing had:** a union of any types,
  `[?a integer]`; an intersection, `(& A B)`; an open record,
  `{n: integer ?a ...}`; a record with a field set or taken out,
  `(?a with n: integer)`, `(?a without title)`; attributes by their
  labels, `{n: integer, title?: text}`; and a recursive type,
  `(rec ?a {next: ?a})`.
- **The solver has unions and intersections of its own** now, not only
  as a variable's bounds. What flows out of a union flows out of each
  member, and what flows into an intersection into each: those are
  exact. The other two directions choose: a type fits a union where it
  fits a member, and an intersection fits where one of its members does
  (records in it are merged first). The members are tried in turn, and
  a try that fails is undone. The choice is greedy, so a claim whose
  members only some later constraint tells apart can be refused though
  it holds; a function's result is checked before its parameters to make
  that rarer. Overloads are what these intersections would need to be
  exact, and they are not a goal.
- **A record of one attribute, written as shown, is read by where it
  is:** a variant where a value flows out, a record that asks for the
  attribute where a value flows in. The two print the same, `{n:
  integer}`, and the reading that is the more general in each place is
  the one inference would have found.
- **A recursive type is a variable bounded by its body both ways,**
  which is how inference makes one; the type the claim declares is not
  rigid there.
- **A claim of a definition that performs may write its whole type,**
  `(claim greet! : ((\ text) : (task {} with: {print})))`, as it is
  shown, besides the result alone.
- **What printing simplifies changed with it:** functions side by side
  merge (`(& ((\ ?a) : ?b) ((\ ?a) : ?c))` is one function), a
  recursive variable that no constructor guards is dropped before
  variables are merged, and setting a field of `{}` is a record.

## Types are shown as they are written

- **The checker shows a type in the syntax a `type` or `claim` writes:**
  `[:a :b]` for a union (Roc's brackets), `(list ?a)` and `(dict text)`,
  `text`, `integer`, `boolean`, `keyword`, `any`, `nothing`, `?a` for a
  variable, and `((\ integer plus: integer) : integer)` for a function.
  The old `|`, `List[a]` and `(Int) -> Int` were a second notation for
  the same things, and `List[a]` could not have stayed beside bracketed
  unions.
- **Functions keep the claim syntax.** An arrow was considered, Roc's
  `a, b -> c`; the claim form stays, and a function that has effects
  still answers a task, `((\ text) : (task {} with: {print}))`.
- **A variable that is a record as well is an open record,**
  `{todo-id: text ?a ...}`: the record `?a`, which has `todo-id`. It is
  Roc's `{ todoId : Str }a`, written with the `...` of a record
  pattern's rest. Unlike Roc's row variable, `?a` is the whole record,
  so a function that answers it answers `?a`.
- **Other intersections are `(& A B)`.** They are rare, a parameter that
  is a variable and a text at once, and have no Roc counterpart.
- **What is shown can be written back** (see above).
- **`any`, `nothing`, `view` and `effect` are type names now,** so the
  types that print them can be written.
- **Errors use the same names, with an article:** `text is not an
  integer`.

## Two arguments: the command and the model

- **`update` takes the command and the model; `view` takes the model.**
  The attributes were a third argument to both. They are now what Elm
  makes of a program's inputs: `init` sees the first ones, and each
  change is a command, `{action: :attribute-changed attribute: :title
  value: "Buy milk"}` (the attribute a keyword, the value absent when it
  was removed). The model keeps what the element needs of them, typed:
  `todo-item`'s `done` is a `Bool`, where the attribute is the string
  `"true"`.
- **The cost is a copy:** the attribute lives in the host's element and
  again in the model, and an element that forgets a clause for an
  attribute shows a stale value. Every element here has one clause per
  observed attribute. Putting the attributes in the command instead
  would have kept one copy, but `view`, which hears no command, would
  still have needed them as an argument.
- **A record pattern's `rest ...` is the rest of the record:**
  `{action: :toggle-all params ...}` binds `params` to the command
  without `action`, as `[x rest ...]` binds a list without `x`. (It
  first bound the whole record, which read wrong: `{editing: true model
  ...}` named something that still had `editing`.) Its type is the
  narrowed record with each named field taken out, `(r without
  editing)`: an update that sets the field to nothing, `{k?: Nothing}`,
  so reading it is an error and an optional read finds nothing, with no
  new kind of type in the solver. At run time a new record is made
  without the fields. There is still no pattern that binds a value whole
  while testing it; the clauses here have not needed one since.
- **Display merges variables that every input has together.** Each
  clause's `assoc` left a variable of its own, all of them in the
  parameter's intersection and each in its own `with` in the result.
  When two variables appear in negative positions only together,
  instantiating both with what flows in is as general as keeping them
  apart, so display merges them (MLsub's co-occurrence analysis, for the
  negative polarity alone). Updates of an update show as one, `(b with
  height: a, phase: String)`, and setting a field twice as once.
  `update` went from eight variables to one.

## Commands with keyword actions

- **`update` hears a command: `(update command model: m attrs: a)`.**
  The handler was a separate string parameter; it is now the command's
  `action` field, a keyword, so a clause matches it with the rest of the
  event: `(update {action: :keydown key: "Enter" value: title} model:
  {editing: true} attrs)`.
- **The command comes first,** as Elm's `update msg model` has it, so a
  clause's pattern for it goes without a label. The model is labeled.
- **Actions are keywords, as in Clojure,** wherever code names one: a
  view's events, `{:click :toggle}`, and a request's, `(fetch .. :loaded)`.
  `h` and `fetch` take keywords there. The host still names handlers
  with strings, so the glue turns the string it is given into a keyword
  (one just lifted, so its kind is set in place); the lifecycle actions
  `:connected` and `:attribute-changed` are static keywords.
- **Handler names became keyword-shaped:** `toggleAll` is
  `:toggle-all`, and `todo-item`'s `key` is `:keydown`, so the action
  and the `key` attribute do not read alike.
- **Where a clause matched a field to test it, it binds the field to use
  it,** `{action: :toggle value: which}`, since there is still no
  pattern that binds the whole command. A command without the field
  takes the catch-all instead of reading `""`; `todo-input`'s
  `:toggle-all` keeps its old meaning with a clause for `checked: true`
  and one for the rest.

## Elements' `update` by clauses

- **Every element's `update` is now clauses,** one per handler, with the
  model and the event matched where that says something: `{editing:
  true}`, `event: {key: "Enter" value: title}`. The trial was whether
  the no-overlap rule gets in the way. It did not: dispatch on a handler
  string makes clauses disjoint, and the catch-all is wider than all of
  them.
- **What the trial cost:**
  - Each clause repeats the whole head, `(define (update model handler:
    "x" event attrs) ..)`; `todo-app` has fifteen.
  - Four parameters are matched as one record, allocated per call:
    `todo-item.wasm` grew from 25411 to 27467 bytes.
  - `update`'s inferred model type is wider: `{editing: true}` reads the
    field only where it is there, so the parameter is `a`, not
    `{editing: Bool}`.
- **What it found missing:**
  - `true` and `false` were not patterns; they are now, and the two
    cover a `Bool` (and a parameter where clauses name only them must
    be one).
  - There is no pattern that both tests a value and binds it whole
    (Haskell's `x@p`, core.match's `:as`). A clause that matched
    `event: {key: "Enter"}` could not then pass `event` on, so it binds
    the field it needs, `{key: "Enter" value: title}`. A key event
    without a value now takes the next clause rather than reading `""`.

## Functions by clauses

- **Every `define` of a name in a compilation is a clause of one
  function,** a set as a macro's rules are: the narrowest clause that
  matches a call is taken, overlap with neither narrower is an error,
  and the function's hash does not depend on the clauses' order (they
  are sorted by their encoding, then ordered narrowest first). Defining
  a value twice is an error, where the later used to replace the
  earlier.
- **A set has no first, so clauses that agree must still not overlap.**
  `(zip [] ys)` and `(zip xs ys: [])` both match `(zip [] ys: [])` and
  would answer the same, but neither is narrower, so it is an error;
  `(zip [x xs ...] ys: [])` says it. This is the cost of order meaning
  nothing; deciding that two bodies agree is not decidable in general.
- **List patterns use the macros' `...`.** `[x rest ...]` binds `rest`
  to the list of the rest. In a value pattern `...` follows a name or
  `_` only: a repeated pattern such as `[(ok v) ...]` needs a loop in
  the compiled test, and is left for later.
- **Several parameters are matched as one record** of the arguments,
  keyed `conduct.argument/000` and on, so `match` and its narrowing
  needed nothing new. It allocates a record per call; a decision tree
  would not, and is an optimization for later.
- **Coverage is Maranget's usefulness over the parameters together.** A
  column (a parameter) in which no clause matches anything is taken to
  hold only the variants it names, and the type checker requires that of
  the argument; any other column may hold anything, so the clauses that
  match anything there must cover the rest. Lists are enumerated by
  length; strings, integers and records need a clause that matches
  anything.
- **A parameter's name binds,** even one naming a record type, unlike a
  bare name in a `match`, which tests for the type. A function's
  parameter called `person` should not stop matching anything.
- **The first parameter's label** is the name the clauses that name it
  give it, and they must agree; with none, it is `value`.
- **A variant's rank orders the record types it ranks,** in clauses as
  in a `match`: `(report (done ..))`, `(report (pending ..))` and
  `(report (idle ..))` overlap, neither narrower, but `job` ranks them.
- **What a review found, and these fix:**
  - A keyword and a variant of the same key in one parameter were taken
    for one case, so clauses could pass as covering and leave a call
    unmatched. They are now different cases.
  - Records with different fields were taken as disjoint, as variants
    are, even where an argument may be any record; clauses could then be
    ambiguous with no error. They are disjoint only in a parameter that
    holds just the variants its clauses name.
  - A field every clause names in one parameter forced the argument to
    be that variant, where a single parameter accepts any record with
    the field. Now both accept it.
- **A list pattern in a list pattern makes every item a list.** Items
  share one type, and the compiled test reads items without checking
  what kind of value they are, so `[x [y] z]` beside `["a" b]` is a type
  error; telling kinds apart at run time would allow it.
- **`rec a. a & T` is shown as `T`.** Recursion through a clause's `rest`
  left a variable bounded by itself; an unguarded occurrence of a
  recursive type's variable says nothing, so it is dropped when the type
  is shown.

## Macros

- **Rules, not functions, first.** A rule is a pattern and a template,
  as Scheme's `syntax-rules` and Rust's `macro_rules!` are. Nothing runs
  at compile time, so no interpreter, fuel or purity question; most of
  what a macro needs (`cond`, `and`, `or`, `when`) is substitution.
  Procedural macros, functions over a Lux-style variant of forms, come
  when a macro needs to compute (a new name from an old one, say). A
  draft of their interpreter is in this branch's history.
- **Each rule is a form with its pattern in the head,** and a macro is
  the set of its rules, as a Datalog predicate is. Racket's and Rust's
  rules are a list tried in order; here order means nothing, so the
  macro's hash is of its rules sorted.
- **The narrowest rule wins; overlap without one is an error.** A set
  needs a rule for two rules that both match. Datalog keeps every
  match; an expansion needs one form. Narrower (every call it matches,
  the other matches too) is decided on the patterns when the set is
  closed, as a ranked variant's cases are checked. `...` comes last in
  a form's patterns, which keeps that decision structural.
- **A compilation closes a macro.** Every rule of a name in a source,
  with the codebase's earlier rules for it, is one macro, closed before
  anything in the source expands. Code compiled before keeps its
  expansions: a hash-addressed codebase does not re-evaluate the whole
  program as Datalog does, so a rule added later makes a new macro.
- **Hygiene by marks and pins.** Every symbol a template writes, other
  than a pattern variable, carries the expansion's mark, a character no
  source can contain. A local the template binds is then distinct from
  any the call binds, and a marked name no local binds is resolved
  globally, skipping the call's locals. When the macro is closed, each
  such symbol naming a definition is pinned to its hash, which is what
  Clojure's syntax-quote does with namespaces. Racket's full sets of
  scopes are not needed for rules macros; symbols a procedural macro
  builds will need a decision.
- **Patterns match labels as written, in order.** A call's labels may
  come in any order; a macro's pattern says which order it takes.
  Normalizing labels before matching is possible later.
- **A macro is closed before the source's definitions are added,** so
  a template's names are resolved by name in its own compilation and
  pinned at its end.
- **`and`, `or` and `cond` moved to `core.cdt`.** `Term::And` and
  `Term::Or` are no longer produced; inference and code generation
  still handle them, and they can go in a cleanup.
- **A macro's expansion cannot write a macro rule yet;** that needs
  closing rules to a fixpoint.
- **A local may not take a macro's name.** Expansion runs before names
  are resolved and does not see locals, so `(define (f sq) (sq 3))`
  would expand the macro `sq`. Refusing the local keeps "a name is one
  thing"; `and`, `or` and `cond` are no longer reserved words but are
  macros, so they cannot name locals either.
- **What a review found, and these fix:**
  - A parameter a template wrote took its marked name as its label,
    and labels are hashed, so a definition's hash depended on how many
    expansions ran before it. Labels are now the bare name.
  - The reader accepted the mark and pin characters, so source could
    capture a template's binder or call a shadowed definition by hash.
    A name with a control character is now a read error.
  - Adding the same source again saw its pinned rule and its unpinned
    copy as two rules matching the same calls. Rules are now told apart
    as written, and the first, already pinned, is kept.
  - A definition a template writes pinned its own recursive call to the
    definition it replaced. The name a template defines is not pinned.
  - A macro `foo!` and a definition `foo` could both exist, the macro
    hiding the task. Each now refuses the other.
  - Errors showed marks. Forms are shown as written.
- **Expansion is at most 512 macro calls deep.** A recursive rule adds
  one level per step, so `(and ..)` of hundreds of forms fails where the
  old built-in did not. Rules cannot report errors of their own, so
  `(cond :else 1 x 2)` says no rule matches `(cond)`, not that `:else`
  must come last; procedural macros will be able to say so.

## Types and dictionaries

- **`type` declares what a name is; `claim` is only a signature.** One
  form for both meant a claim's meaning depended on whether a `define`
  of its name happened to be in the same source, and a misspelled
  definition name silently turned a signature into a type. Now a claim
  with no definition in its source is an error that suggests `type`, and
  a type of a defined name is an error that suggests `claim`. This
  replaces the rule under Claims below.
- **A record type's members are attributes, never record types.**
  `(type line : {point})` is an error: a member names a key, and a
  record type is not one. A record holds another through an attribute
  that holds it, `(type start : (io.gozala.line/start point))`. The
  alternative, splicing `point`'s attributes in, would make `{point}`
  mean two different things by what `point` is.
- **Attributes written in place read as a record writes its values,**
  `{io.gozala.point/x: integer}`, besides `{(io.gozala.point/x
  integer)}`.
- **A dictionary's keys are keywords,** `{:class "x"}`; string-keyed
  literals are an error that says what to write. Keywords are a new
  `Keyword` type that every keyword without a payload fits. At run time
  a keyword and a string with the same bytes are the same key, so a
  dictionary the host builds with strings is read with keywords.
- **Entries are kept sorted by the keys' bytes,** in a literal (sorted
  when parsed), by `assoc` (a sorted insert in the runtime), and in the
  glue that builds an element's attributes. So a dictionary is one
  value however it was built, and its hash, its equality and its JSON
  do not depend on order. JSON a dictionary encodes now lists its keys
  sorted; the routes' responses changed accordingly.
- **`get` reads both containers:** `(get key of: container or:
  default)`. A key that is a declared attribute's name (not a local)
  reads the attribute, exactly as `(name record)` does, with `or:`
  only where the record may lack it. Anything else is a dictionary key
  and needs `or:`. An attribute is never a computed value, so which
  read it is is decided when the call is resolved. Calling an attribute,
  `(title todo)`, stays.
- **The elements' `attr` helper is gone;** `(get :filter of: attrs
  or: "")` says the same.

## Tasks

- **Calls are pure; `!` performs.** A definition marked `!` makes a
  task; `(f! x)` performs what `(f x)` answers; `(t!)` performs a task
  held in a value, since a definition with no parameters is a value.
  The naming rule (a name ends with `!` exactly when it uses
  capabilities) is gone: `!` is never part of a name, so the check is
  syntax, and a function given to `map` needs no effect polymorphism.
- **A task's body is the only place that performs.** A function's body
  is pure even inside a task, so `(map xs each: (fn [x] (log! x)))` is
  an error, and `fn!` makes a function that answers a task.
- **Arrows are pure.** `Ty::Fn` keeps its effect slot, always pure now;
  capabilities live on `Ty::Task`. Taking the slot out is mechanical and
  left for a cleanup.
- **A task is a closure of no parameters,** and performing one calls
  it. Performing a definition's task where it is made calls a performing
  entry compiled for it, so `(stamp! x)` makes no task. Elements' and
  routes' memory tests run unchanged.
- **Environments are tail-resumptive handlers.** An implementation
  answers its operation's result and the task goes on, so no coroutines
  or continuations are needed: a host call looks up the innermost
  environment that provides its capability, a stack of records the
  runtime keeps, and calls the implementation in the import's place.
  Handlers that abort or resume later need coroutines, and are not here.
- **Implementations are pure.** One that performs would make the
  environment need capabilities of its own; the type rule for that is
  simple, but running the implementation's task in the right
  environment is the coroutine work above.
- **Subtracting capabilities without negation types.** `(perform t
  with: env)` needs `t`'s capabilities less `env`'s labels. A
  `CapsPlus(rest, provided)` upper bound says it: a set flowing into it
  flows into `rest` once `provided` is taken out. The labels are always
  written out, so nothing needs MLstruct's negation.
- **`{}` is both an empty dictionary and a record with no attributes.**
  Implementations and functions with nothing to answer need an empty
  record, and `{}` already meant an empty dictionary. Its type,
  `Prim::Empty`, fits both; at run time it is an empty dictionary, which
  record operations treat as a record with no fields.
- **Host functions are named without `!`,** and `add_host` refuses a
  name with one: a call performs them with it, as any task.
- **An independent review found, and these fix:**
  - `assoc` with an attribute never required a record, so `(assoc 1
    name: "x")` type checked and corrupted memory. Its input must now
    fit `{}`; this bug predates tasks.
  - An implementation was checked inside the task's body, so it could
    perform, and recursed forever into its own environment. It is now
    checked as pure, and an operation of no parameters is given its
    answer once, not a task re-run at each call. Which operations take
    none came from looking a name up at compile time, which broke when
    the name was later rebound; it now comes from the call.
  - `{}` from the host (an empty record) and from source (an empty
    dictionary) were unequal, and the record encoded as `null`. Empty
    records and dictionaries now compare equal and encode as `{}`.
  - A ranked match without `_` was sound but could reject values its
    own constructors built. It must now name every case, and the last
    case must be in every other; otherwise it needs `_`.
  - A capability or operation given twice in an environment is an
    error, as an attribute given twice is.
- **A call with no arguments is the value itself,** as the note says:
  `(now)` is `now`. No function takes none, so nothing is lost.
- **A record type is a value** when it has no optional attributes: its
  constructor, `(map xs each: ok)`. With optional ones, different calls
  pass different labels, which a function value cannot.

## Claims

- **A claim with a `define` of the same name, in the same source, is a
  signature; any other defines a type.** The note left this open; the
  rule is the simplest that tells the two apart without a second form.
  An attribute's claim of a defined name is an error, not a signature.
- **Signatures are checked with rigid variables.** A claim's `?a` is a
  `Ty::Rigid` while the definition is checked, a type that fits only
  itself, so a body that assumes anything of it fails. Callers then see
  the claim with its variables free. Simple-sub has no rigid variables
  of its own; one new type that is a subtype only of itself (and `Any`)
  was enough.
- **Callers see the claim, and the claim is part of the hash.** Two
  definitions with one term and different claims are different
  definitions, since their callers see different types.
- **Capabilities a signature leaves out are what the body uses:** the
  capabilities the check found, and those of the function parameters,
  which are the claim's own variables. Linking the claim to the whole
  inferred type instead was sound but showed recursive types.
- **A record type of one attribute is a variant,** as a record literal
  of one attribute already was, and is shown as its constructor:
  `(ok Int)`. `ok`, `error` and `result` are claimed in `core.cdt`; `ok`
  is no longer an attribute, so `{ok: x}` is `(ok x)`.
- **A match over a ranked variant's cases is sorted by rank** when it
  is resolved. Once ranked the cases are disjoint, so sorting changes
  no result, and first-match order then classifies by rank with no new
  pattern form. Records of one attribute need no rank: their key tells
  them apart.
- **Ranking needs record types, and a variant's union is their join:**
  attributes every case has are required, the others optional. That is
  what a `job` parameter accepts.
- **Recursive types are refused** for now: expanding a claim that
  refers to itself stops at a depth and says so.
- **A bare name in a pattern that names a record type tests for it;**
  any other name binds. Types and definitions share one namespace, so
  the reading is never ambiguous for a definition's name.

## Labels and attributes

- **`define` replaces `def` and `defn`;** both say what to write
  instead. A definition with no parameters is a value, so
  `(define (f) ..)` is an error now, and the place tasks will use later.
- **One unlabeled argument, the first.** Every other argument is passed
  by its parameter's name, and the first may be too. Labels are resolved
  to positions when the call is resolved: the first parameter, then the
  rest sorted by label. So code generation is unchanged, and a labeled
  call costs what a positional one does.
- **Labels are part of a function's type and its hash.** A call through
  a function value can only be checked against labels the type carries,
  and a definition's labels, the first's included, are its interface:
  renaming one breaks callers. Renaming a local (a `let`, or a label's
  separate local name) still keeps the hash.
- **Labels must match exactly** when a function value is called. Width
  subtyping on labels would let a caller pass a label the function
  ignores, and the positions would be wrong.
- **Built-ins stay positional,** except `map` (`each:`) and `filter`
  (`where:`), which the design's examples use. The others move into the
  prelude as conduct definitions eventually, and take labels then.
- **Records are keyed by global names.** A key must be a claimed
  attribute, by its local name or its global one. The term holds the
  global name, so the hash and the field id do not depend on what code
  calls it, and types are shown with local names again.
- **An attribute's claimed type checks what sets it,** in a record
  literal or an `assoc`. Reads are not constrained by it: a read's type
  comes from what was stored, which the claim already checked.
- **A `?a` in an attribute's type asks nothing yet.** `ok` and `error`
  are claimed `(conduct.result/ok ?value)` until claims with type
  constructors arrive; then `(ok ?a)` ties the variable to its use.
- **A name is one thing.** An attribute's local name cannot also name a
  definition, so `(name person)` always reads an attribute. A local
  shadows it, as locals shadow definitions.
- **The platform claims its contract's attributes** in a prelude per
  world (`element.cdt`, `route.cdt`), and every codebase starts with
  `core.cdt`. The response status is claimed as `code`, since `status`
  is the prelude's function that builds a response.

## No nil

- **No `nil` in the language.** Roc's case against Maybe (it does not
  say why) and Hickey's "Maybe Not" (optionality belongs to the
  context) point at different uses of "maybe"; each gets its own form.
  Failure is `{:ok v} | {:error why}`; meaningful absence is a variant
  that names it; an attribute that may be missing is absent from the
  record, typed `k?: t`.
- **Optional fields, not optional values.** `Opt(t)` is only ever a
  record field's type. Reading one with `(:k r)` is a type error; it
  takes a default, `(:k r d)`, or a `match`. The default form desugars
  to that match, so it needed no new term.
- **Optional fields and width subtyping.** A record typed without a
  field may, through width subtyping, have it at run time with another
  type, so trusting an absent field to stay absent would be unsound in
  general. Simple-sub keeps every literal's own type as a lower bound,
  so the precise type is what gets checked; only a declared type that
  forgets fields (a host's) could hide one, and the glue builds exact
  records. Revisit when annotations arrive.
- **`get` and `first` take a default** rather than answer a variant:
  the default is almost always what the caller wants next.
- **A WIT function with `option` parameters takes one record of its
  named parameters.** Lisp cannot leave out a positional argument, and
  WIT names its parameters. Applied by hand to `update!` so far; the
  general rule belongs with world bindings.
- **Nothing to answer is `{}`,** Roc's empty record.
- **JSON keeps an opaque null.** A `Json` may be null (or missing, or
  malformed), seen through `json-type`; no other value can be. The
  absent value still exists inside the runtime (0x02) for glue slots
  and JSON, never reachable as a conduct value of another type.
- **Removed:** `nil`, `nil?`, `??`, `if-some`, the `nil` and `some?`
  patterns, and the solver's `t \ nil` and `t | nil`. The forms give an
  error that says what to write instead.
- **Displayed types say more:** a union of records shows a field one of
  them lacks as `k?:`, rather than dropping it.

## Unions

- **Variants are records of one field,** as asked: `{:ok t}`. Keywords
  are the variants without a payload, so `:ok | {:error e}` is one type.
  The keyword-set lattice became a variant-set lattice: inclusion of
  keys, payloads covariant.
- **A record literal of one field is always a variant.** Records keep
  width subtyping, which is what makes two records with different
  fields join to their shared fields; a variant has to be a type of its
  own to join without loss. It reads as a record, so existing code that
  uses one-field records as models still type checks. Alternative:
  syntax that marks a variant. Reverse if one-field records that are
  meant as records start to confuse inference.
- **One payload per variant.** Records are the product type; WIT's
  variants have one payload too.
- **Patterns bind names.** A symbol in a pattern binds; the name is not
  in the hash, as a parameter's is not.
- **`{k?: t}`, an upper bound only, types a pattern's read.** It is
  CUE's optional field: a value without `k` fits. So a record pattern
  no longer requires its field everywhere: with a `_` clause, `(match
  job {:status :active} ..)` accepts anything. The requirement comes
  back from exhaustiveness when there is no `_`.
- **Exhaustiveness is per key.** Without `_`, a match on variants
  constrains its value to exactly those keys, each payload to what its
  patterns cover. When every clause names the same single key, that is
  read as a record field (`{:status :active}` / `{:status :suspended}`),
  which also accepts variants with that key.
- **Openness needs no rows.** Roc writes `[A, B]*` to keep a union
  open; with subtyping a match with `_` simply puts no upper bound on
  its value.
- **`assoc` of a variant's own key is still that variant;** any other
  `assoc` makes a record.
- **Display:** a lone variant beside records is shown as the record it
  is, so element models print as before.

## Routes

- **Routes call the model directly.** An element returns effects as
  data; a route's `handle!` uses the `model` capability and the route
  contract allows no other. This is the Roc split the effects slice set
  up: a pure core, effects at the edge that the type names.
- **Blocking, not callbacks, for the model.** The model's functions are
  `async`; the glue calls them with the synchronous ABI and lets the
  task block, so code generation stays direct-style. The cost: the host
  must be able to suspend a guest. Wasmtime and the polyfill can; a
  stackless host could not run a route. The alternative, CPS-converting
  `handle!` at each effectful call, would make every capability call a
  suspension point in generated code.
- **Async-lowered streams and futures, waited on one at a time.** Their
  synchronous forms need the component model's "more async builtins"
  feature, which Wasmtime and the polyfill leave off by default. Each
  wait uses a waitable set of its own, so two requests in flight never
  take each other's events.
- **`t | nil` as a type former** (superseded by No nil: `update!` takes
  a record whose `title` may be absent). Needed to declare `update!`'s
  optional parameters; Simple-sub's unions only appear where values
  come out.
  It is safe to solve because nil is disjoint from every other head
  type. Two spellings of an optional now exist (an inferred union and
  this), and they display the same. If source annotations come, they
  would produce this one.
- **Results as records with optional fields** (superseded by Unions:
  results are now `{:ok v} | {:error e}`).
- **`pattern` must be a string literal,** read at compile time, so the
  glue matches it with code specific to it.
- **The glue adds `x-demo-route`,** as the Zena authoring library's
  `serve` does, so the demo's page can show which route answered.
- **A JSON title that is not a string becomes `""`.** `json-str` answers
  a neutral value, and there is no way to ask a `Json` its kind, so a
  PATCH with `{"title": 5}` answers 422 where the Zena route ignores the
  title. A `json-string?` (or narrowing on `Json`) would fix it.
- **`cabi_realloc` allocates nothing for nothing.** Wasmtime asks for
  room for an empty list; the glue frees no empty buffer, so it would
  leak. A request for zero bytes now gets a fixed address.

## Effects

- **A capability set on every function type.** Roc tracks only pure or
  effectful; conduct infers which capabilities, as a set ordered by
  inclusion, so a type says what a function can reach. The empty set is
  pure.
- **Effects are a third part of the function type,** covariant like the
  result, so the solver needed only a new lattice of sets, not a new
  kind of constraint. A call adds the callee's effect to the body it is
  in; `map` and `filter` take their function's.
- **Effect polymorphism is kept.** `apply` is `((a) => b ! c, a) => b ! c`.
  Roc avoids it; inference gives it for free, and the naming rule lets
  such a function go without `!`.
- **The `!` rule is an error both ways,** where Roc warns. A pure
  function named with `!`, or an effectful one without, is refused.
- **Host functions are declared through the API for now.** A world's
  imports become such declarations when routes need them; there is no
  syntax for declaring one yet.
- **Effects run directly.** Code generation does not change: an
  effectful call is a call. Waiting on an async import blocks the task
  (see Routes).

## Reuse: a bug it found

- **Record fields are evaluated in source order.** A record literal
  stores its fields sorted by id, and it used to evaluate them in that
  order too, while ownership was worked out in source order. A local
  used in two fields could then be moved by the field evaluated first
  and read after it was freed. Found by the reuse tests; fixed by
  evaluating into temporaries first.

## Slice 5: keywords and match

- **A keyword is its own type, and keyword sets form a lattice by
  inclusion.** No general unions are needed for them, and the solver
  checks one set against another directly.
- **Exhaustiveness by construction.** A `match` without `:else`
  constrains its value to what its patterns cover, so a missing case is
  an error where an uncovered value flows in. With no annotations there
  is nothing else to check the match against; a Dialog schema's listed
  domain could be that something later.
- **`_`, not `:else`, ends a `match`.** In Dialog's notation a keyword
  is an entity (`:uri` is `uri:`), so `:else` must stay a value a match
  can test for. `_` is the default clause and, in a record pattern, a
  field that only has to be there. It is required wherever the patterns
  cannot cover (strings, integers, mixed shapes). `cond` still ends with
  `:else`, which it reads as syntax; it should follow.
- **Narrowing from patterns, not predicates.** Each clause narrows the
  local it tested, directly or through a path of fields, by typing the
  narrowed fields as a record update does. Narrowing from user-defined
  predicates would need propositions in function types (Typed Racket's
  occurrence typing). Named patterns are the cheaper way to name a shape.
- **A clause learns nothing from the ones before it.** That needs
  negation types. With exhaustive matches on one field, it rarely
  matters.
- **Keywords print with their colon in `str` and as their name in
  JSON.** A view attribute wanting the bare name has to match on it.

## Slice 4: requests

- **The async ABI's callback form, not blocking.** A synchronously
  lifted export could call `send` and block on it, which is less glue.
  But blocking needs stack switching: Wasmtime has fibers, while the
  polyfill needs JSPI in the browser and fails without it. The callback
  form needs neither, and it lets the instance serve other calls while a
  request is in flight. The glue keeps each request as a small state
  machine that the callback advances. Only the exports that run
  `update` are async; the rest stay synchronous.
- **A task ends when its requests do.** `handle-event` returns after
  every request it caused, including the reload a change asks for, has
  reached `update`. That matches the Zena element, which `await`s its
  requests, and the demo's page, which renders after each call. The
  alternative, returning at once and telling the host to render later,
  needs something in the world to say so.
- **Relative URLs are left to the host.** The demo's page resolves a
  path against its origin. Wasmtime cannot and refuses a request with no
  authority, so outside the browser a URL must be absolute. A base URL
  for elements is the host's business, not the element's.
- **A failed `send` reaches `update` as status 0 with no body.** The
  error code is dropped, and any string in it is not freed yet.
- **The test host vendors the demo's `wasi:http/types` (MPL-2.0).** Its
  modules are private to the demo crate. Copying them tests conduct's
  elements against the host the demo's page really uses.
- **A debug world for tests.** `element::debug_component` adds an export
  `conduct:debug/memory#live`, so a test through the component model can
  count the blocks an element holds.

## Slice 3: todo-app's logic

- **Effects are data, and requests are effects.** `update` returns
  `{model, effects}`, where an effect is an `emit` or a `fetch`. A
  `fetch` names the handler its response comes back to, so a request and
  its answer are two calls of a pure `update`, as in Elm. This holds for
  elements; routes, which must wait on the database inside one request,
  still need a real effect system.
- **The lifecycle goes through `update`.** `connected` and
  `attribute-changed` reach `update` as events, so `<todo-app>` loads
  its list from `update` like everything else, instead of the glue
  knowing about loading.
- **One `Json` type with lenient accessors.** `json-get`, `json-str`,
  `json-int`, `json-bool` and `json-items` answer a neutral value when
  the document has another shape, like the Zena demo's helpers. No
  decoders and no sum types needed; a wrong shape is not an error. Typed
  decoding can come with sum types.
- **`nil <: Json`.** JSON's `null` is nil, so `(first items)` (a
  `Json | nil`) is a `Json`.
- **`edit`'s detail is `id\ntitle`,** as `<todo-item>` sends it, and
  `<todo-app>` takes the title from the second line.
- **Field reads through updates are cached by field and type.** Reading
  a field through `r with k set` builds a one-field record for `r`;
  since that record is new each time, the constraint cache keyed by
  address missed it, and a model flowing back into its own update sent
  the contract check around forever. Reads are now cached by `r`, the
  field and the wanted type.
- **Open: big `update`s get big types.** `<todo-app>`'s `update` infers
  a principal type that is correct and unreadable: a union of chains of
  updates over intersected variables. The simplifier does not merge
  update chains with the records they meet. Worth fixing before types
  are shown to anyone writing a route.

## Slice 2: todo-input, todo-footer, record update

- **Built-ins resolve after bindings.** A call to a name no local or
  definition binds may name a built-in, so bindings shadow built-ins
  instead of built-in names being reserved. A definition already hashed
  keeps calling the built-in if a same-named definition appears later.
- **Record update may change a field's type.** `(assoc r :k v)` is typed
  with a positive-only type former, "r with k set", that answers a field
  read from `v` for `k` and from `r` otherwise. This gives the rest of
  the record through, like row polymorphism, without row variables.
- **`todo-input`'s clear flag lives in the model.** The Zena element
  clears it inside `render`; a pure view cannot, so the next keystroke
  clears it instead. The rendered result is the same.

## Reference counting (Perceus without reuse)

- **Owned calling convention.** Compiled functions consume their
  arguments and return owned values; closure bodies borrow what they
  captured. This is Perceus's convention and needs no annotations.
- **Built-ins borrow.** Runtime functions borrow their arguments and
  return owned results, and the code generator drops temporaries after
  each call. Simpler to write the runtime by hand; costs a temporary per
  argument. Revisit when reuse analysis lands, since reuse needs
  built-ins that can consume.
- **Reuse is decided at run time, by the count.** The updating
  built-ins consume their collection and change it in place when its
  count is one. Koka also pairs a drop with an allocation of the same
  size at compile time, so a constructor can reuse a block whose last
  use came just before; that needs pattern matching on constructors,
  which conduct does not have yet, and the run-time check covers the
  updates that matter now.
- **`conj` on a unique list doubles its block** when the block is full,
  so repeated `conj` is amortized constant time. A list may hold unused
  capacity; nothing reads past its length.
- **`conduct_allocs`,** an export beside `conduct_live`, counts every
  allocation, so tests can check that reuse happens, not just that
  results agree.
- **Free lists by size class, poisoned on free.** Poisoning costs a
  `memory.fill` per free and makes use-after-free bugs fail loudly. Make
  it a debug option once the tests stop finding such bugs.
- **Lent results are released in `post-return`,** and every export also
  releases at entry, in case a host never calls `post-return`.
- **An extra `conduct_live` core export** counts live blocks for tests.
  wit-component accepts it; it does not appear in the component's
  interface.

## Iteration 1

- **Hash the term, not the type.** With full inference a type follows
  from the term and its dependencies, so it is derived data. Keeps
  hashes stable while the type system changes.
- **Simple-sub plus `t \ nil`** (superseded by No nil). Algebraic subtyping for unions and
  intersections with principal inference; one targeted negation (nil
  removal) for set-style optionals instead of full MLstruct negation.
- **Effects as data in elements.** `update` returns custom events and
  the glue sends them, so element code is pure. Routes will need real
  effects.
- **Sync lift and lower.** The world's `async` exports are lifted
  synchronously and `host.emit` is lowered synchronously, keeping the
  async ABI out of the compiler. Revisit for `wasi:http` and the async
  model imports.
- **WAT as the codegen target,** assembled with the `wat` crate. Easy to
  read and debug; slower to assemble than encoding binary directly.
- **Data-derived tags (proposed, not built).** Variants whose tag a set
  of ranked rules derives from the data, aligned with Dialog's listed
  domains under the `top` pick. Open: whether conduct also needs
  constructor tags, which would need negation types.
