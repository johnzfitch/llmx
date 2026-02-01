/**
 * Sample TypeScript module for testing.
 */

interface User {
    id: number;
    name: string;
    email: string;
}

type UserRole = 'admin' | 'user' | 'guest';

/**
 * User service for managing users.
 */
class UserService {
    private users: Map<number, User> = new Map();

    constructor() {}

    addUser(user: User): void {
        this.users.set(user.id, user);
    }

    getUser(id: number): User | undefined {
        return this.users.get(id);
    }

    getAllUsers(): User[] {
        return Array.from(this.users.values());
    }
}

// Generic function example
function identity<T>(arg: T): T {
    return arg;
}

// Type guard example
function isUser(obj: unknown): obj is User {
    return typeof obj === 'object' && obj !== null && 'id' in obj && 'name' in obj;
}

export { User, UserRole, UserService, identity, isUser };
