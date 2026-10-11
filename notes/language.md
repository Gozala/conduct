# Conduct: a proposal

A synthesis of the design discussion, written to be built. Each section
ends with what changes in the compiler. Syntax marked *provisional* is a
placeholder for a decision still open (listed at the end).

## Principles, in the order they win

1. **Values are plain, immutable data.** Records, lists, strings,
   numbers, keywords. There is no `nil` value: absence is not a thing a
   program can hold.
2. **Types are inferred; declarations are optional constraints.** A
   program with no declarations is fully typed. A declaration narrows
   what inference found; it never replaces it.
3. **Type declarations are rules that describe, never rules that
   choose.** A rule constrains and identifies data. No rule ever decides
   which code runs, so there are no type classes, no coherence or orphan
   rules, and rules may be spread across a program freely.
4. **Names are fully qualified; rules give local names.** Data always
   carries `:io.gozala/name`; a rule binds it to `name` where you say so.
5. **Effects are inferred and visible**, as in Roc. So is failure.
6. **The core is small; the rest is macros.** Macros are pure,
   hygienic by hash, and can read what the compiler knows.
7. **A definition is its hash**, taken after macro expansion, over a
   name-free term. Types and expansions are derived data.

When two principles conflict, the earlier one wins.

## 1. Values

```clojure
{:io.gozala/name "Alice" :io.gozala/age 28}   ; a record: qualified keyword keys
[1 2 3]                                       ; a list
:active                                       ; a keyword, the type of just itself
"text" 42
```

Keys are fully qualified keywords. An unqualified key is an error in a
record literal (dictionaries keep string keys).

**Compiler:** the reader accepts `:a.b/c`; record literals require a
namespace. Small.

## 2. Absence is failure

No expression evaluates to "nothing". Reading something that may be
missing *fails*, and failure is only allowed where something handles it:

```clojure
(?? (first titles) "untitled")                       ; fallback
(if-some [s (:io.gozala/since job)] (str "Since " s) "not started")
(match (:io.gozala/since job) some? ... _ ...)
```

- `(first xs)`, `(get d "k")`, and reading an optional field are
  failable.
- A failable expression anywhere else is a compile error that says where
  absence must be handled. `[(first xs)]` does not compile: a list
  cannot have a hole.
- A function whose body can fail without handling it can fail. "Can
  fail" is inferred and shown on the function's type, like an effect,
  so `find-todo` simply fails and its callers handle it.
- At the machine level a failable call returns a status. No program can
  store, compare, or pass it: unobservable, which is what
  "unrepresentable" means here.
- JSON's `null` is the keyword `:null`, a value. Missing and null stay
  distinct.

This is Verse's failure, and the functional face of a logic query with
no answers.

**Compiler:** the biggest change. `nil` leaves the value space and the
type lattice; `Without` and `t | nil` become a "fails" flag on
expressions and function types, checked at failure contexts. Event
fields in the element contract become absent instead of `nil`.

## 3. Type rules

A type is a rule, in the style of SICP's query language: a conclusion
and a body of premises that must all hold. Several rules with one
conclusion are alternatives.

```clojure
(rule (person ?p ?name ?age)                 ; provisional: `rule`
  (the io.gozala/name of: ?p is: ?name)
  (the io.gozala/age of: ?p is: ?age)
  (type ?name :text)                         ; provisional spelling
  (type ?age :integer))

(rule (status ?job :complete)
  (the io.example/completed-at of: ?job is: _))

(rule (status ?job :pending)
  (the io.example/created-at of: ?job is: _))
```

### What a rule body may contain

| Premise | Meaning | At runtime |
|---|---|---|
| `(the k of: ?v is: ?x)` | `?v` has key `k`, whose value is `?x` | key present |
| `(type ?x :text)` | primitive type constraint | type test |
| `(= ?x :active)`, `(= ?x 3)` | literal | equality |
| `(person ?x ...)` | another rule | its test |
| `(= ?y (f ?x))` | `?y` has `f`'s result type | **none** |
| bounds: `(>= ?age 0)` | refinement | comparison |

The function premise is used for inference only: `f`'s signature
contributes a type, and `f` never runs. No negation in the first
version. Recursion is allowed, solved with tabling and a depth limit,
since type terms nest.

### How a rule is used

One rule is three things, depending on where it appears:

- **A type.** A rule's conclusion is a relation whose rows are records:
  a `person` is `{p, name, age}` with the types its premises give. The
  type checker solves rules like Rust's trait solver solves clauses
  (Chalk, the new solver), as Horn clauses over types.
- **A constructor and a destructuring**, when its body is only `the`
  and `type` premises over its head variables. Head variables double as
  labels, which is how a rule spares you the full key names:

  ```clojure
  (person :name "Alice" :age 28)       ; => {:io.gozala/name "Alice" :io.gozala/age 28}
  (let [(person p name age) value] (str name ", " age))
  ```

- **A pattern**, when every premise has a runtime test (the right
  column). The same rule then narrows in `match`:

  ```clojure
  (match job
    (status :complete) "done"
    (status :pending) "waiting")       ; exhaustive: the rules' tags
  ```

  A rule with a function premise is a type only. Using it as a pattern
  is a compile error, because there is nothing to run.

Rules with constant heads (`:complete`, `:pending`) are tagged
alternatives: the union of their tags is a closed set, so a `match` on
them is checked for exhaustiveness. Untagged alternatives of the same
rule are allowed as types but cannot be matched exhaustively.

### Ranking

When a value satisfies several alternatives and code needs one answer,
the first alternative in the conclusion's listed order wins. The order is
declared once (provisional):

```clojure
(rank status :complete :pending)
```

so clauses can live anywhere without their position mattering.

**Compiler:** a `rule` form; lowering bodies to constraints in the
existing solver (fields and keyword sets already exist there); a small
Horn-clause solver for rule references, with tabling; constructors,
destructuring, and pattern compilation for rules that qualify.

## 4. Functions and match

As today: `define`, `fn`, `let`, `if`, `match` with `_` as the last
clause, narrowing in each clause. `match` patterns gain rule patterns
(above) and lose `nil` (it is `_` or a failure context now).

## 5. Effects

Roc's model, with capabilities:

- A function that calls an effectful function is effectful. Its type
  uses `=>`, and its name ends in `!`.
- The set of capabilities a function reaches is part of its type, so a
  type says what a function can touch. An empty set is pure.
- Only the host provides effectful functions (a component's imports).
- Failure (section 2) is inferred the same way and shown beside effects.

Effectful calls run directly, blocking on their own stack, where the
host can suspend (Wasmtime; JSPI in browsers). Elements keep effects as
data in `update`, which needs none of this.

## 6. Macros

Two tiers, the first one first:

1. **Syntax macros.** A macro is a pure conduct function from forms to a
   form, run by the compiler. Hygiene comes from hash addressing: a
   symbol in a macro's template resolves to a hash where the *macro* is
   defined; binders it introduces are fresh. Macros can read what the
   compiler knows (a definition's type, a rule's premises), as Lux
   macros run in `Meta` with the compiler's state. `cond`, `and`, `or`,
   `if-some`, `defn` move out of the core into macros.
2. **Typed quotes** later, after Chris Done's sketch: `~e` has type
   `'T`, so a macro that only splices its arguments (`when`, `->`)
   can be checked at its call, before expansion.

jerd (Jared Forsyth) planned a third arrangement that combines the two:
macros receive a *typed* syntax tree and the typing environment, and
"all nodes will be parsed and type-checked before going in, unless
explicitly quoted". Typed by default, quoted by choice. Its catch is the
same one that puts tier 1 first here: a macro that introduces binders
(`defn`, `match`) cannot have its arguments typed before it runs, so in
jerd those arguments would all be quoted. Which default conduct takes,
typed with quoting by choice or raw with typing by choice, can wait
until tier 2; tier 1 is needed either way.

The compiler runs macros with an interpreter for the pure subset, so
macros work wherever the compiler does, browser included; tests run
each macro compiled too, to catch drift.

A definition's hash is taken after expansion; the source as written and
the macro hashes that expanded it are kept beside it.

## 7. Plan

Each step is a pull request with tests; each keeps the four elements
working.

1. **Qualified keys.** Reader, record literals, the examples renamed.
2. **Failure instead of `nil`.** Value space, solver, codegen, the
   element contract, examples. Every existing test is rewritten against
   the new semantics, which is the measure of whether it reads better.
3. **Rules as types.** `rule`, the premise table above, the solver for
   rule references, rules as types in inference. Patterns and
   constructors second.
4. **Macros, tier 1.** A `form` value type, quote and syntax-quote,
   `defmacro`, the interpreter, and `cond`/`and`/`or`/`defn` re-done as
   macros.
5. **Effects.** Purity and capability inference, `=>` and `!`, then the
   demo's route with `wasi:http/handler` on Wasmtime.

The acceptance test for the whole is four programs written as you would
want to read them: the todo element (exists), the `/api/todos` route, a
view over changing data, and a function of the kind a rule would call.

## Open decisions

1. **Premise spelling.** `(type ?x :text)` collides with naming the
   rule form `type`; this draft names the form `rule`. Alternatives:
   `(is ?x :text)`, or `(:text ?x)`.
2. **A failing value in a record literal.** Omit the key silently
   (`{:k (first xs)}` has no `:k` when `xs` is empty), or require
   handling the failure explicitly. This draft requires it.
3. **Marking failable functions in their names**, as `!` marks effects
   (`find-todo?`), or in their types only.
4. **Signatures.** With rules as types, is `(: label (-> status :text))`
   still needed, or do rule patterns in parameters cover it?
5. **Ranking syntax.** `(rank status ...)`, or the listed order of the
   first rule.
6. **Macro default, for tier 2.** Arguments typed before expansion
   unless quoted (jerd), or raw unless marked typed (Chris Done's `~`).
