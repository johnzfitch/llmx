/**
 * Sample C file for testing.
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define MAX_NAME_LEN 64
#define MAX_USERS 100

/* User structure */
typedef struct {
    int id;
    char name[MAX_NAME_LEN];
    int active;
} User;

/* User database */
static User users[MAX_USERS];
static int user_count = 0;

/**
 * Add a new user to the database.
 * @param name The user's name
 * @return The user's ID, or -1 on error
 */
int add_user(const char* name) {
    if (user_count >= MAX_USERS) {
        return -1;
    }

    User* user = &users[user_count];
    user->id = user_count;
    strncpy(user->name, name, MAX_NAME_LEN - 1);
    user->name[MAX_NAME_LEN - 1] = '\0';
    user->active = 1;

    return user_count++;
}

/**
 * Find a user by ID.
 * @param id The user's ID
 * @return Pointer to user, or NULL if not found
 */
User* find_user(int id) {
    if (id < 0 || id >= user_count) {
        return NULL;
    }
    return &users[id];
}

/**
 * Print all users.
 */
void print_users(void) {
    printf("Users (%d total):\n", user_count);
    for (int i = 0; i < user_count; i++) {
        printf("  [%d] %s (active: %d)\n",
               users[i].id, users[i].name, users[i].active);
    }
}

int main(void) {
    add_user("Alice");
    add_user("Bob");
    add_user("Charlie");

    print_users();

    User* user = find_user(1);
    if (user) {
        printf("Found: %s\n", user->name);
    }

    return 0;
}
