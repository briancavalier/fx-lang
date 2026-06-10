# Phase 2.6 executable fixture entrypoint.

import support

export main() : Text
  piped =
    "Grace" |> append-suffix("!") |> wrap("[", "]")

  single =
    with surround("(", ")")
      piped

  stacked =
    with "ctx" |> surround-value("outer")
    with surround("inner(", ")")
      "body"

  single ++ " " ++ stacked
