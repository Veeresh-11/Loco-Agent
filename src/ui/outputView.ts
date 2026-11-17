import * as vscode from 'vscode';

export class OutputView {
  private channel: vscode.OutputChannel;

  constructor(name: string = 'LOCO') {
    this.channel = vscode.window.createOutputChannel(name);
  }

  log(message: string) {
    this.channel.appendLine(message);
  }

  show() {
    this.channel.show();
  }

  clear() {
    this.channel.clear();
  }
}
