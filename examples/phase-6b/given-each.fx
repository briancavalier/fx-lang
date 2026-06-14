type Clock = { now: Text }

stamp(message: Text) : Text given { clock: Clock }
  clock.now ++ message

export main() : Text
  given each(() -> { clock: { now: "10:00" } })
    stamp(" started")
