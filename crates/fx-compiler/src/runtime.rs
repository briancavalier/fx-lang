pub const JAVASCRIPT_RUNTIME: &str = r#"
const __fx = Object.freeze({
  variant(tag, ...payload) {
    return Object.freeze({ __fx_variant: true, tag, payload });
  },

  isVariant(value, tag) {
    return Boolean(value && value.__fx_variant === true && value.tag === tag);
  },

  matchVariant(value, cases, fallback) {
    if (!value || value.__fx_variant !== true) {
      throw new TypeError("fx match expected a variant value");
    }
    const branch = cases[value.tag];
    if (branch !== undefined) {
      return branch(...value.payload);
    }
    if (fallback !== undefined) {
      return fallback(value);
    }
    throw new Error(`fx match missing case ${value.tag}`);
  },

  list(...items) {
    return Object.freeze(items.slice());
  },

  listIsEmpty(value) {
    return value.length === 0;
  },

  listHead(value) {
    if (value.length === 0) {
      throw new Error("fx list head on empty list");
    }
    return value[0];
  },

  listTail(value) {
    if (value.length === 0) {
      throw new Error("fx list tail on empty list");
    }
    return Object.freeze(value.slice(1));
  },

  equal(left, right) {
    if (Array.isArray(left) && Array.isArray(right)) {
      return left.length === right.length && left.every((item, index) => __fx.equal(item, right[index]));
    }
    return left === right;
  },

  effect(run) {
    return Object.freeze({
      __fx_effect: true,
      run,
    });
  },

  pure(value) {
    return __fx.effect((_handler, continuation) => continuation(value));
  },

  perform(operation, payload) {
    return __fx.effect((handler, continuation) => handler({
      operation,
      payload,
      resume(value) {
        return continuation(value);
      },
    }));
  },

  bind(computation, continuation) {
    return __fx.effect((handler, finalContinuation) =>
      __fx.runEffect(computation, handler, (value) =>
        __fx.runEffect(continuation(value), handler, finalContinuation)
      )
    );
  },

  runEffect(computation, handler, continuation) {
    if (computation && computation.__fx_effect === true) {
      return computation.run(handler, continuation);
    }
    return continuation(computation);
  },

  handle(action, spec) {
    return __fx.effect((outerHandler, continuation) => {
      const localHandler = (request) => {
        const clause = spec.operations[request.operation];
        if (clause === undefined) {
          return outerHandler(request);
        }
        const resume = (value) => __fx.runEffect(request.resume(value), localHandler, continuation);
        return __fx.runEffect(clause(...request.payload, resume), localHandler, continuation);
      };
      return __fx.runEffect(action, localHandler, (value) =>
        __fx.runEffect(spec.return(value), outerHandler, continuation)
      );
    });
  },

  run(computation) {
    return __fx.runEffect(
      computation,
      (request) => {
        throw new Error(`Unhandled fx operation ${request.operation}`);
      },
      (value) => value
    );
  },
});
"#;

pub fn javascript_runtime() -> &'static str {
    JAVASCRIPT_RUNTIME
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn runtime_constructs_and_matches_variants() {
        let output = run_node(
            r#"
const found = __fx.variant("Found", "Grace");
const missing = __fx.variant("Missing");
const rendered = [
  __fx.isVariant(found, "Found"),
  __fx.matchVariant(found, { Found: (name) => `Hello, ${name}` }),
  __fx.matchVariant(missing, { Found: (name) => name }, () => "missing")
];
console.log(JSON.stringify(rendered));
"#,
        );

        assert_eq!(output, r#"[true,"Hello, Grace","missing"]"#);
    }

    #[test]
    fn runtime_reports_missing_match_cases() {
        let output = run_node(
            r#"
try {
  __fx.matchVariant(__fx.variant("Other"), {});
} catch (error) {
  console.log(error.message);
}
"#,
        );

        assert_eq!(output, "fx match missing case Other");
    }

    #[test]
    fn runtime_supports_list_helpers_and_equality() {
        let output = run_node(
            r#"
const items = __fx.list("Grace", "Ada");
const tail = __fx.listTail(items);
console.log(JSON.stringify({
  empty: __fx.listIsEmpty(items),
  head: __fx.listHead(items),
  tailHead: __fx.listHead(tail),
  equalLists: __fx.equal(items, __fx.list("Grace", "Ada")),
  equalText: __fx.equal("phase", "phase")
}));
"#,
        );

        assert_eq!(
            output,
            r#"{"empty":false,"head":"Grace","tailHead":"Ada","equalLists":true,"equalText":true}"#
        );
    }

    #[test]
    fn runtime_helpers_are_isolated_under_fx_namespace() {
        let output = run_node(
            r#"
console.log(JSON.stringify({
  hasNamespace: typeof __fx === "object",
  hasGlobalVariant: typeof variant,
  namespaceFrozen: Object.isFrozen(__fx)
}));
"#,
        );

        assert_eq!(
            output,
            r#"{"hasNamespace":true,"hasGlobalVariant":"undefined","namespaceFrozen":true}"#
        );
    }

    #[test]
    fn runtime_handles_operation_requests_and_returns() {
        let output = run_node(
            r#"
const program = __fx.handle(
  __fx.perform("main.raise", ["divide by zero"]),
  {
    operations: {
      "main.raise": (_message, _resume) => __fx.pure(0),
    },
    return(value) {
      return __fx.pure(value);
    },
  },
);
console.log(__fx.run(program));
"#,
        );

        assert_eq!(output, "0");
    }

    #[test]
    fn runtime_resumes_operation_continuations() {
        let output = run_node(
            r#"
const program = __fx.handle(
  __fx.bind(__fx.perform("main.choose", [1, 2]), (value) => __fx.pure(value + 10)),
  {
    operations: {
      "main.choose": (left, _right, resume) => resume(left),
    },
    return(value) {
      return __fx.pure(value);
    },
  },
);
console.log(__fx.run(program));
"#,
        );

        assert_eq!(output, "11");
    }

    #[test]
    fn runtime_reports_unhandled_operations() {
        let output = run_node(
            r#"
try {
  __fx.run(__fx.perform("main.raise", ["divide by zero"]));
} catch (error) {
  console.log(error.message);
}
"#,
        );

        assert_eq!(output, "Unhandled fx operation main.raise");
    }

    fn run_node(script: &str) -> String {
        let output = Command::new("node")
            .arg("-e")
            .arg(format!("{JAVASCRIPT_RUNTIME}\n{script}"))
            .output()
            .expect("node runs runtime test");

        assert!(
            output.status.success(),
            "node failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );

        String::from_utf8(output.stdout)
            .expect("utf8 stdout")
            .trim()
            .to_string()
    }
}
