interface FormData {
  email: string;
  age: number;
  name: string;
}

function isValidEmail(email: string): boolean {
  return /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email);
}

function isValidAge(age: number): boolean {
  return Number.isInteger(age) && age >= 0 && age <= 150;
}

export function validateForm(data: FormData): string[] {
  const errors: string[] = [];
  if (!isValidEmail(data.email)) {
    errors.push("Invalid email address");
  }
  if (!isValidAge(data.age)) {
    errors.push("Age must be between 0 and 150");
  }
  if (data.name.trim().length === 0) {
    errors.push("Name is required");
  }
  return errors;
}
