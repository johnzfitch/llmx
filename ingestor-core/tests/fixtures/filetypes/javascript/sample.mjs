/**
 * Sample ES module for testing.
 */

export const PI = 3.14159;

export function circleArea(radius) {
    return PI * radius * radius;
}

export function circleCircumference(radius) {
    return 2 * PI * radius;
}

export default class Circle {
    constructor(radius) {
        this.radius = radius;
    }

    get area() {
        return circleArea(this.radius);
    }

    get circumference() {
        return circleCircumference(this.radius);
    }
}
