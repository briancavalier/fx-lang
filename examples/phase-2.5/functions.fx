# Phase 2.5 executable fixture support module.
#
# This module focuses on functions and lambdas without introducing later-phase
# runtime concepts.

export apply-twice(f: Text -> Text, value: Text) : Text
  f(f(value))

export make-suffix(suffix: Text) : Text -> Text
  value -> value ++ suffix

export decorate(value: Text) : Text
  suffix = make-suffix("!")
  apply-twice(suffix, value)

export computed() : Text
  "phase" ++ " 2.5"
