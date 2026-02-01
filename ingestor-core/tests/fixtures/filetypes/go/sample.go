// Package sample provides example Go code for testing.
package sample

import (
	"context"
	"fmt"
	"sync"
)

// User represents a user in the system.
type User struct {
	ID    int
	Name  string
	Email string
}

// UserService manages user operations.
type UserService struct {
	mu    sync.RWMutex
	users map[int]*User
}

// NewUserService creates a new UserService.
func NewUserService() *UserService {
	return &UserService{
		users: make(map[int]*User),
	}
}

// AddUser adds a user to the service.
func (s *UserService) AddUser(user *User) error {
	s.mu.Lock()
	defer s.mu.Unlock()

	if _, exists := s.users[user.ID]; exists {
		return fmt.Errorf("user %d already exists", user.ID)
	}

	s.users[user.ID] = user
	return nil
}

// GetUser retrieves a user by ID.
func (s *UserService) GetUser(id int) (*User, bool) {
	s.mu.RLock()
	defer s.mu.RUnlock()

	user, ok := s.users[id]
	return user, ok
}

// ListUsers returns all users.
func (s *UserService) ListUsers() []*User {
	s.mu.RLock()
	defer s.mu.RUnlock()

	users := make([]*User, 0, len(s.users))
	for _, user := range s.users {
		users = append(users, user)
	}
	return users
}

// FetchData simulates an async data fetch operation.
func FetchData(ctx context.Context, url string) (string, error) {
	select {
	case <-ctx.Done():
		return "", ctx.Err()
	default:
		return fmt.Sprintf("Data from %s", url), nil
	}
}

// Calculator provides basic math operations.
type Calculator interface {
	Add(a, b int) int
	Subtract(a, b int) int
	Multiply(a, b int) int
}

type basicCalculator struct{}

func (c *basicCalculator) Add(a, b int) int      { return a + b }
func (c *basicCalculator) Subtract(a, b int) int { return a - b }
func (c *basicCalculator) Multiply(a, b int) int { return a * b }

// NewCalculator creates a new Calculator.
func NewCalculator() Calculator {
	return &basicCalculator{}
}
