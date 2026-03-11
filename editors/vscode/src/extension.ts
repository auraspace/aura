import * as path from 'path';
import { workspace, ExtensionContext } from 'vscode';
import {
    LanguageClient,
    LanguageClientOptions,
    ServerOptions,
    Executable
} from 'vscode-languageclient/node';

let client: LanguageClient;

export function activate(context: ExtensionContext) {
    // The server is implemented in rust
    // We launch it with the --lsp flag
    
    // For development, we assume the binary is in the workspace root or built via cargo
    // You might want to allow configuring this path in settings
    let serverPath = workspace.getConfiguration('aura').get<string>('serverPath') || 'aura';

    const run: Executable = {
        command: serverPath,
        args: ['--lsp'],
        options: {
            env: {
                ...process.env,
                RUST_LOG: 'debug'
            }
        }
    };

    const serverOptions: ServerOptions = {
        run,
        debug: run
    };

    // Options to control the language client
    const clientOptions: LanguageClientOptions = {
        // Register the server for aura files
        documentSelector: [{ scheme: 'file', language: 'aura' }],
        synchronize: {
            // Notify the server about file changes to '.aura' files contained in the workspace
            fileEvents: workspace.createFileSystemWatcher('**/*.aura')
        }
    };

    // Create the language client and start the client.
    client = new LanguageClient(
        'auraLanguageServer',
        'Aura Language Server',
        serverOptions,
        clientOptions
    );

    // Start the client. This will also launch the server
    client.start();
}

export function deactivate(): Thenable<void> | undefined {
    if (!client) {
        return undefined;
    }
    return client.stop();
}
