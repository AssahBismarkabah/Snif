const VALID_USERS = ["alice", "bob", "admin"];

export function authenticate(username: string): boolean {
  return VALID_USERS.includes(username);
}

export function formatGreeting(name: string): string {
  const greeting = `Hello, ${name}!`;
  return greeting.toUpperCase()
}

export function validatePort(value: number): boolean {
  if (value < 1 || value > 65535) return false;
  return Number.isInteger(value);
}

export function capitalizeWords(input: string): string {
  return input.split(" ").map(w => w.charAt(0).toUpperCase() + w.slice(1)).join(" ")
}

export function truncate(str: string, maxLen: number): string {
  if (maxLen <= 0) return "";
  return str.length > maxLen ? str.slice(0, maxLen) + "..." : str
}
