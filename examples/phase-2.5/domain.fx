# Phase 2.5 executable fixture support module.
#
# This module keeps to the accepted pure Phase 1 and Phase 2 language while
# exercising records, record update, variants, matches, lists, recursion, and
# open-row consumers for future compiler lowering and JavaScript emission.

export type Person = {
  name: Text,
  done: Bool
}

export type Lookup a = < Found(a), Missing(), Error(Text) >

export greet(person: { name: Text, ...r }) : Text
  "Hello, " ++ person.name

export mark-done(person: Person) : Person
  { ...person, done: true }

export find-person(wanted: Text, people: List Person) : Lookup Person
  match people
    [] ->
      Missing()

    [person, ...rest] ->
      if person.name == wanted then Found(person) else find-person(wanted, rest)

export render(lookup: < Found(Person), Missing(), Error(Text), ...r >) : Text
  match lookup
    Found(person) ->
      greet(person)

    Missing() ->
      "missing"

    Error(text) ->
      text

    other ->
      "unknown"
