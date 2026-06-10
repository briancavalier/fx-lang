# Phase 3 goal target: explicit resumptions in handler clauses.

export effect Choose
  choose(left: a, right: a) : a

choose-first(action: () -> a with <Choose, ...e>) : a with <...e>
  handle action()
    choose(left, _right, resume) ->
      resume(left)
    return(value) ->
      value

choose-second(action: () -> a with <Choose, ...e>) : a with <...e>
  handle action()
    choose(_left, right, resume) ->
      resume(right)
    return(value) ->
      value

export main() : Int
  with choose-first()
    choose(1, 2) + 10
