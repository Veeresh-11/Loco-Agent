import * as vscode from 'vscode';

export class OutputView {
  private readonly channel: vscode.OutputChannel;

  constructor(name: string = 'LOCO') {
    this.channel = vscode.window.createOutputChannel(name);
  }

  log(message: string) {
    this.channel.appendLine(message);
  }

  get outputChannel(): vscode.OutputChannel {
    return this.channel;
  }

  show() {
    this.channel.show();
  }

  clear() {
    this.channel.clear();
  }

  dispose() {
    this.channel.dispose();
  }
}
