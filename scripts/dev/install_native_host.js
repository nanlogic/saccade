function shellQuote(value) {
  return `'${String(value).replace(/'/g, `'"'"'`)}'`;
}

function run(arguments) {
  if (arguments.length !== 2) {
    throw new Error('expected manifest source and destination directory');
  }
  const sourcePath = arguments[0];
  const destinationDirectory = arguments[1];
  const destinationPath = `${destinationDirectory}/com.nanlogic.saccade.dev.json`;
  const command = [
    `/usr/bin/install -d -m 755 ${shellQuote(destinationDirectory)}`,
    `/usr/bin/install -m 644 ${shellQuote(sourcePath)} ${shellQuote(destinationPath)}`,
  ].join(' && ');
  const application = Application.currentApplication();
  application.includeStandardAdditions = true;
  application.doShellScript(command, { administratorPrivileges: true });
}
