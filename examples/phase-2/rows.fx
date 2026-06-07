# Phase 2 row-polymorphism acceptance fixture.
#
# This file is intentionally compact and covers the expected-passing open-record
# and open-variant behavior for Phase 2.

greet(person: { name: Text, ...r }) : Text
  "Hello, " ++ person.name

message(result: < Ok(a), Missing(), Error(Text), ...r >) : Text
  match result
    Ok(_) -> "ok"

    Missing() -> "missing"

    Error(text) -> text

    other -> "unknown"

# Boundary goals for future row diagnostics:
#
# Missing field:
#
# missing-field(person: { name: Text, ...r }) : Text
#   person.title
#
# Unknown variant case:
#
# unknown-case(result: < Ok(Text), ...r >) : Text
#   match result
#     Failure(text) -> text
#     other -> "unknown"
#
# Duplicate record label:
#
# duplicate-record(person: { name: Text, name: Int, ...r }) : Text
#   person.name
#
# Duplicate variant label:
#
# duplicate-variant(result: < Ok(Text), Ok(Int), ...r >) : Text
#   "duplicate"
#
# Incompatible row kind:
#
# row-kind-mismatch(person: { name: Text, ...r }, result: < Ok(Text), ...r >) : Text
#   message(person)
