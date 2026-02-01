"""
Sample Python module for testing.

This module demonstrates various Python constructs for indexing tests.
"""

from dataclasses import dataclass
from typing import List, Optional, Dict
import asyncio


@dataclass
class User:
    """Represents a user in the system."""
    id: int
    name: str
    email: str
    active: bool = True


class UserService:
    """Service for managing users."""

    def __init__(self):
        self._users: Dict[int, User] = {}

    def add_user(self, user: User) -> None:
        """Add a user to the service."""
        self._users[user.id] = user

    def get_user(self, user_id: int) -> Optional[User]:
        """Get a user by ID."""
        return self._users.get(user_id)

    def list_users(self) -> List[User]:
        """List all users."""
        return list(self._users.values())

    def delete_user(self, user_id: int) -> bool:
        """Delete a user by ID."""
        if user_id in self._users:
            del self._users[user_id]
            return True
        return False


def calculate_statistics(numbers: List[float]) -> Dict[str, float]:
    """Calculate basic statistics for a list of numbers."""
    if not numbers:
        return {"count": 0, "sum": 0, "mean": 0, "min": 0, "max": 0}

    return {
        "count": len(numbers),
        "sum": sum(numbers),
        "mean": sum(numbers) / len(numbers),
        "min": min(numbers),
        "max": max(numbers),
    }


async def fetch_data(url: str) -> str:
    """Async function to simulate data fetching."""
    await asyncio.sleep(0.1)
    return f"Data from {url}"


if __name__ == "__main__":
    service = UserService()
    service.add_user(User(1, "Alice", "alice@example.com"))
    print(service.list_users())
