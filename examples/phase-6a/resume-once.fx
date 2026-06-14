# Phase 6A: a resumed continuation stays under the same deep handler boundary.

export effect Ask
  ask() : Text

ask-twice() : Text with <Ask>
  first = ask()
  second = ask()
  first ++ second

answer(action: () -> Text with <Ask, ...e>) : Text with <...e>
  handle action()
    ask(resume) ->
      resume("x")
    return(value) ->
      value

export main() : Text
  with answer()
    ask-twice()
