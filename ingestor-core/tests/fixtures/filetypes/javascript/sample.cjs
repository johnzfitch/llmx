/**
 * Sample CommonJS module for testing.
 */

const VERSION = '1.0.0';

function createLogger(prefix) {
    return {
        info: (msg) => console.log(`[${prefix}] INFO: ${msg}`),
        warn: (msg) => console.warn(`[${prefix}] WARN: ${msg}`),
        error: (msg) => console.error(`[${prefix}] ERROR: ${msg}`),
    };
}

class Config {
    constructor(defaults = {}) {
        this.values = { ...defaults };
    }

    get(key) {
        return this.values[key];
    }

    set(key, value) {
        this.values[key] = value;
    }
}

module.exports = { VERSION, createLogger, Config };
