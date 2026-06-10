type Todo = { id: Int, title: Text, done: Bool }

type TodoResult a = < Ok(a), NotFound(Int) >

summarize-result(result: TodoResult Todo) : Text
  match result
    Ok({ title, done }) -> if done then title ++ " [done]" else title ++ " [open]"
    NotFound(id) -> "missing"

first-title(todos: List Todo) : Text
  match todos
    [] -> "none"
    [todo, ...rest] -> todo.title

bool-label(value: Bool) : Text
  match value
    true -> "yes"
    false -> "no"
