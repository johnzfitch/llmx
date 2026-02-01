/**
 * Sample C++ file for testing.
 */

#include <iostream>
#include <vector>
#include <string>
#include <memory>
#include <unordered_map>

namespace sample {

/**
 * User class representing a system user.
 */
class User {
public:
    User(int id, std::string name)
        : id_(id), name_(std::move(name)), active_(true) {}

    int id() const { return id_; }
    const std::string& name() const { return name_; }
    bool active() const { return active_; }

    void deactivate() { active_ = false; }

private:
    int id_;
    std::string name_;
    bool active_;
};

/**
 * Service for managing users.
 */
class UserService {
public:
    /**
     * Add a new user.
     * @param name The user's name
     * @return The new user's ID
     */
    int addUser(const std::string& name) {
        int id = static_cast<int>(users_.size());
        users_.emplace(id, std::make_unique<User>(id, name));
        return id;
    }

    /**
     * Get a user by ID.
     * @param id The user's ID
     * @return Pointer to user, or nullptr if not found
     */
    User* getUser(int id) {
        auto it = users_.find(id);
        return it != users_.end() ? it->second.get() : nullptr;
    }

    /**
     * List all users.
     * @return Vector of user pointers
     */
    std::vector<User*> listUsers() const {
        std::vector<User*> result;
        result.reserve(users_.size());
        for (const auto& [id, user] : users_) {
            result.push_back(user.get());
        }
        return result;
    }

private:
    std::unordered_map<int, std::unique_ptr<User>> users_;
};

} // namespace sample

// Template example
template<typename T>
T add(T a, T b) {
    return a + b;
}

int main() {
    sample::UserService service;

    service.addUser("Alice");
    service.addUser("Bob");

    auto users = service.listUsers();
    for (const auto* user : users) {
        std::cout << "User: " << user->name() << " (ID: " << user->id() << ")\n";
    }

    std::cout << "Sum: " << add(10, 20) << "\n";

    return 0;
}
