# Phase 2.6 pipeline and prefix-with support module.

export append-suffix(suffix: Text, value: Text) : Text
  value ++ suffix

export wrap(prefix: Text, suffix: Text, value: Text) : Text
  prefix ++ value ++ suffix

export surround(prefix: Text, suffix: Text, action: () -> Text) : Text
  prefix ++ action() ++ suffix

export surround-value(prefix: Text, value: Text, action: () -> Text) : Text
  prefix ++ value ++ ":" ++ action()
