type JsonRecord = Record<string, unknown>;

const defaults: JsonRecord = { theme: "light", language: "en" };

function mergeInto(target: JsonRecord, source: JsonRecord): void {
  for (const [key, value] of Object.entries(source)) {
    if (value && typeof value === "object" && !Array.isArray(value)) {
      const nestedTarget =
        typeof target[key] === "object" && target[key] !== null
          ? (target[key] as JsonRecord)
          : ((target[key] = {}) as JsonRecord);
      mergeInto(nestedTarget, value as JsonRecord);
      continue;
    }

    target[key] = value;
  }
}

export function mergeConfig(userInput: JsonRecord): JsonRecord {
  const config: JsonRecord = {};
  mergeInto(config, defaults);
  mergeInto(config, userInput);
  return config;
}

export function mergeConfigSafe(userInput: JsonRecord): JsonRecord {
  return {
    theme: typeof userInput.theme === "string" ? userInput.theme : defaults.theme,
    language: typeof userInput.language === "string" ? userInput.language : defaults.language,
  };
}
