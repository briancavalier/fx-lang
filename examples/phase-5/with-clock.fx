export type Clock = { now: Text }

with-clock(clock: Clock, action: () -> a given { clock: Clock, ...c }) : a given { ...c }
  given { clock = clock }
    action()

timestamp(message: Text) : Text given { clock: Clock }
  clock.now ++ " " ++ message

export main() : Text
  with with-clock({ now: "00:00" })
    timestamp("started")
