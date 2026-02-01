-- Sample SQL file for testing
-- Contains various SQL statements and patterns

-- Create users table
CREATE TABLE IF NOT EXISTS users (
    id SERIAL PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    email VARCHAR(255) UNIQUE NOT NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    active BOOLEAN DEFAULT TRUE
);

-- Create index on email
CREATE INDEX idx_users_email ON users(email);

-- Create posts table with foreign key
CREATE TABLE IF NOT EXISTS posts (
    id SERIAL PRIMARY KEY,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    title VARCHAR(255) NOT NULL,
    content TEXT,
    published BOOLEAN DEFAULT FALSE,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Insert sample data
INSERT INTO users (name, email) VALUES
    ('Alice', 'alice@example.com'),
    ('Bob', 'bob@example.com'),
    ('Charlie', 'charlie@example.com');

-- Query: Get all active users
SELECT id, name, email
FROM users
WHERE active = TRUE
ORDER BY created_at DESC;

-- Query: Get posts with user info
SELECT
    p.id,
    p.title,
    p.content,
    u.name AS author_name,
    p.created_at
FROM posts p
INNER JOIN users u ON p.user_id = u.id
WHERE p.published = TRUE
ORDER BY p.created_at DESC
LIMIT 10;

-- Function: Update user
CREATE OR REPLACE FUNCTION update_user(
    p_id INTEGER,
    p_name VARCHAR(255),
    p_email VARCHAR(255)
)
RETURNS VOID AS $$
BEGIN
    UPDATE users
    SET name = p_name,
        email = p_email,
        updated_at = CURRENT_TIMESTAMP
    WHERE id = p_id;
END;
$$ LANGUAGE plpgsql;

-- View: User statistics
CREATE OR REPLACE VIEW user_stats AS
SELECT
    u.id,
    u.name,
    COUNT(p.id) AS post_count,
    MAX(p.created_at) AS last_post_at
FROM users u
LEFT JOIN posts p ON u.id = p.user_id
GROUP BY u.id, u.name;
