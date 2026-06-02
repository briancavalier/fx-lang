import * as path from "node:path";
import * as vscode from "vscode";
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
} from "vscode-languageclient/node";

let client: LanguageClient | undefined;

export function activate(context: vscode.ExtensionContext): void {
  const repositoryRoot = path.resolve(context.extensionPath, "..", "..");
  const serverOptions: ServerOptions = {
    command: "cargo",
    args: ["run", "-q", "-p", "fx-lsp", "--bin", "fx-lsp"],
    options: {
      cwd: repositoryRoot,
    },
  };

  const clientOptions: LanguageClientOptions = {
    documentSelector: [{ scheme: "file", language: "fx" }],
    synchronize: {
      fileEvents: vscode.workspace.createFileSystemWatcher("**/*.fx"),
    },
  };

  client = new LanguageClient(
    "fx",
    "fx Language Server",
    serverOptions,
    clientOptions,
  );

  client.start();
  context.subscriptions.push({
    dispose: () => {
      void client?.stop();
    },
  });
}

export function deactivate(): Thenable<void> | undefined {
  return client?.stop();
}
