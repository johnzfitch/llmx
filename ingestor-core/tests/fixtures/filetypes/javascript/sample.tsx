/**
 * Sample React TypeScript component for testing.
 */
import React, { useState, useEffect } from 'react';

interface ButtonProps {
    label: string;
    onClick: () => void;
    disabled?: boolean;
}

/**
 * A reusable button component.
 */
const Button: React.FC<ButtonProps> = ({ label, onClick, disabled = false }) => {
    return (
        <button
            className="btn"
            onClick={onClick}
            disabled={disabled}
        >
            {label}
        </button>
    );
};

interface CounterProps {
    initialCount?: number;
}

/**
 * A counter component with hooks.
 */
function Counter({ initialCount = 0 }: CounterProps) {
    const [count, setCount] = useState(initialCount);

    useEffect(() => {
        document.title = `Count: ${count}`;
    }, [count]);

    return (
        <div className="counter">
            <span>Count: {count}</span>
            <Button label="Increment" onClick={() => setCount(c => c + 1)} />
            <Button label="Decrement" onClick={() => setCount(c => c - 1)} />
        </div>
    );
}

export { Button, Counter };
export type { ButtonProps, CounterProps };
