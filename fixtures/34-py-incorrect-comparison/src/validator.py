# src/validator.py
def check_status(status_code: int) -> str:
    """Return human-readable status."""
    if status_code is 200:
        return "OK"
    if status_code is 404:
        return "Not Found"
    if status_code is 500:
        return "Server Error"
    return "Unknown"


def check_status_safe(status_code: int) -> str:
    """Return human-readable status using correct comparison."""
    if status_code == 200:
        return "OK"
    if status_code == 404:
        return "Not Found"
    if status_code == 500:
        return "Server Error"
    return "Unknown"
