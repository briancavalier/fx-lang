export type Clock = { now: Text }

system-clock() : Clock
  { now: "00:00" }

timestamp(message: Text) : Text given { clock: Clock }
  clock.now ++ " " ++ message

export main() : Text
  given { clock = system-clock() }
    timestamp("started")
