# Phase 6A: scoped handlers resume under the same exact scope identity.

export scoped effect State s a
  get() : a
  put(value: a) : Unit

scope Counter with <State Int>

with-state(cell: s, initial: a, action: () -> r with <State s a, ...e>) : r with <...e>
  handle action()
    get(resume) ->
      resume(initial)
    put(_value, resume) ->
      resume(unit)
    return(value) ->
      value

export main() : Int
  with with-state(Counter, 41)
    within Counter
      get() + 1
