# Phase 6A: handler nesting order remains observable.

export effect Ask
  ask() : Text

outer(action: () -> Text with <Ask>) : Text
  handle action()
    ask(resume) ->
      resume("outer")
    return(value) ->
      value

inner(action: () -> Text with <Ask>) : Text
  handle action()
    ask(resume) ->
      resume("inner")
    return(value) ->
      value

export main() : Text
  first =
    with outer()
      with inner()
        ask()
  second =
    with inner()
      with outer()
        ask()
  first ++ "/" ++ second
