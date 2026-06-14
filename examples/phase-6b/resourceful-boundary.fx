type Trace = { id: Text }

start-trace() : Trace
  { id: "trace" }

end-trace(_trace: Trace) : Unit
  unit

action() : Text given { trace: Trace }
  trace.id

export main() : Text
  given each(() -> { trace: start-trace() }, context -> end-trace(context.trace))
    action()
