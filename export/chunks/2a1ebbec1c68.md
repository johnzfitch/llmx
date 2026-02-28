---
chunk_index: 1158
ref: "2a1ebbec1c68"
id: "2a1ebbec1c6835b1c05f9a95cd1a3667c1011fd3648813b666f9851b5cfebbb5"
slug: "sample-l1-27"
path: "/home/zack/dev/llmx/ingestor-core/tests/fixtures/filetypes/javascript/sample.mjs"
kind: "text"
lines: [1, 27]
token_estimate: 115
content_sha256: "0637821fe92fe7cbb85b06aec4c8cfee504b9031d668ef9a8c5bc3fea9682a72"
compacted: false
heading_path: []
symbol: null
address: null
asset_path: null
---

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