/**
 * Sample JavaScript module for testing.
 * @module sample
 */

/**
 * A greeting function.
 * @param {string} name - The name to greet.
 * @returns {string} The greeting message.
 */
function greet(name) {
    return `Hello, ${name}!`;
}

/**
 * Calculator class for basic operations.
 */
class Calculator {
    constructor(initialValue = 0) {
        this.value = initialValue;
    }

    add(n) {
        this.value += n;
        return this;
    }

    subtract(n) {
        this.value -= n;
        return this;
    }

    getResult() {
        return this.value;
    }
}

// Arrow function example
const multiply = (a, b) => a * b;

// Async function example
async function fetchData(url) {
    const response = await fetch(url);
    return response.json();
}

module.exports = { greet, Calculator, multiply, fetchData };
