/**
 * Sample React JSX component for testing.
 */
import React, { useState } from 'react';

/**
 * A simple card component.
 */
function Card({ title, children }) {
    return (
        <div className="card">
            <h2 className="card-title">{title}</h2>
            <div className="card-body">{children}</div>
        </div>
    );
}

/**
 * A todo list component.
 */
function TodoList() {
    const [todos, setTodos] = useState([]);
    const [input, setInput] = useState('');

    const addTodo = () => {
        if (input.trim()) {
            setTodos([...todos, { id: Date.now(), text: input, done: false }]);
            setInput('');
        }
    };

    const toggleTodo = (id) => {
        setTodos(todos.map(todo =>
            todo.id === id ? { ...todo, done: !todo.done } : todo
        ));
    };

    return (
        <Card title="Todo List">
            <input
                value={input}
                onChange={(e) => setInput(e.target.value)}
                placeholder="Add a todo"
            />
            <button onClick={addTodo}>Add</button>
            <ul>
                {todos.map(todo => (
                    <li
                        key={todo.id}
                        onClick={() => toggleTodo(todo.id)}
                        style={{ textDecoration: todo.done ? 'line-through' : 'none' }}
                    >
                        {todo.text}
                    </li>
                ))}
            </ul>
        </Card>
    );
}

export { Card, TodoList };
