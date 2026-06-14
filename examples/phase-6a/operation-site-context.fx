# Phase 6A: resume-site providers do not rewrite operation-site context.

type Clock = {
  now: Text
}

export effect Choose
  choose(left: Text, right: Text) : Text

choose-both(action: () -> Text with <Choose> given { clock: Clock }) : Text given { clock: Clock }
  handle action()
    choose(left, right, resume) ->
      given { clock = { now: "resume" } }
        resume(left) ++ "/" ++ resume(right)
    return(value) ->
      value

export main() : Text
  given { clock = { now: "operation" } }
    with choose-both()
      clock.now ++ choose("-left", "-right")
