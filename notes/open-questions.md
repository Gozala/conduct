# Open questions

Decisions taken without waiting, each with what was chosen, why, and what
would reverse it. Newest first. Go over these together; anything marked
**revisit** is a choice that gets more expensive to change the longer it
stands.

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
