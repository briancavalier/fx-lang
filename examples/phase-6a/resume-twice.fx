# Phase 6A: ordinary resumptions are currently multi-resumption-capable.

export effect Choose
  choose(left: Text, right: Text) : Text

choose-both(action: () -> Text with <Choose, ...e>) : Text with <...e>
  label = "branch:"
  handle action()
    choose(left, right, resume) ->
      label ++ resume(left) ++ "|" ++ label ++ resume(right)
    return(value) ->
      value

export main() : Text
  with choose-both()
    choose("left", "right")
