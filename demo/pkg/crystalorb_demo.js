
let wasm;

let cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true, fatal: true });

cachedTextDecoder.decode();

let cachegetUint8Memory0 = null;
function getUint8Memory0() {
    if (cachegetUint8Memory0 === null || cachegetUint8Memory0.buffer !== wasm.memory.buffer) {
        cachegetUint8Memory0 = new Uint8Array(wasm.memory.buffer);
    }
    return cachegetUint8Memory0;
}

function getStringFromWasm0(ptr, len) {
    return cachedTextDecoder.decode(getUint8Memory0().subarray(ptr, ptr + len));
}

const heap = new Array(32).fill(undefined);

heap.push(undefined, null, true, false);

let heap_next = heap.length;

function addHeapObject(obj) {
    if (heap_next === heap.length) heap.push(heap.length + 1);
    const idx = heap_next;
    heap_next = heap[idx];

    heap[idx] = obj;
    return idx;
}

function getObject(idx) { return heap[idx]; }

function dropObject(idx) {
    if (idx < 36) return;
    heap[idx] = heap_next;
    heap_next = idx;
}

function takeObject(idx) {
    const ret = getObject(idx);
    dropObject(idx);
    return ret;
}

function debugString(val) {
    // primitive types
    const type = typeof val;
    if (type == 'number' || type == 'boolean' || val == null) {
        return  `${val}`;
    }
    if (type == 'string') {
        return `"${val}"`;
    }
    if (type == 'symbol') {
        const description = val.description;
        if (description == null) {
            return 'Symbol';
        } else {
            return `Symbol(${description})`;
        }
    }
    if (type == 'function') {
        const name = val.name;
        if (typeof name == 'string' && name.length > 0) {
            return `Function(${name})`;
        } else {
            return 'Function';
        }
    }
    // objects
    if (Array.isArray(val)) {
        const length = val.length;
        let debug = '[';
        if (length > 0) {
            debug += debugString(val[0]);
        }
        for(let i = 1; i < length; i++) {
            debug += ', ' + debugString(val[i]);
        }
        debug += ']';
        return debug;
    }
    // Test for built-in
    const builtInMatches = /\[object ([^\]]+)\]/.exec(toString.call(val));
    let className;
    if (builtInMatches.length > 1) {
        className = builtInMatches[1];
    } else {
        // Failed to match the standard '[object ClassName]'
        return toString.call(val);
    }
    if (className == 'Object') {
        // we're a user defined class or Object
        // JSON.stringify avoids problems with cycles, and is generally much
        // easier than looping through ownProperties of `val`.
        try {
            return 'Object(' + JSON.stringify(val) + ')';
        } catch (_) {
            return 'Object';
        }
    }
    // errors
    if (val instanceof Error) {
        return `${val.name}: ${val.message}\n${val.stack}`;
    }
    // TODO we could test for more things here, like `Set`s and `Map`s.
    return className;
}

let WASM_VECTOR_LEN = 0;

let cachedTextEncoder = new TextEncoder('utf-8');

const encodeString = (typeof cachedTextEncoder.encodeInto === 'function'
    ? function (arg, view) {
    return cachedTextEncoder.encodeInto(arg, view);
}
    : function (arg, view) {
    const buf = cachedTextEncoder.encode(arg);
    view.set(buf);
    return {
        read: arg.length,
        written: buf.length
    };
});

function passStringToWasm0(arg, malloc, realloc) {

    if (realloc === undefined) {
        const buf = cachedTextEncoder.encode(arg);
        const ptr = malloc(buf.length);
        getUint8Memory0().subarray(ptr, ptr + buf.length).set(buf);
        WASM_VECTOR_LEN = buf.length;
        return ptr;
    }

    let len = arg.length;
    let ptr = malloc(len);

    const mem = getUint8Memory0();

    let offset = 0;

    for (; offset < len; offset++) {
        const code = arg.charCodeAt(offset);
        if (code > 0x7F) break;
        mem[ptr + offset] = code;
    }

    if (offset !== len) {
        if (offset !== 0) {
            arg = arg.slice(offset);
        }
        ptr = realloc(ptr, len, len = offset + arg.length * 3);
        const view = getUint8Memory0().subarray(ptr + offset, ptr + len);
        const ret = encodeString(arg, view);

        offset += ret.written;
    }

    WASM_VECTOR_LEN = offset;
    return ptr;
}

let cachegetInt32Memory0 = null;
function getInt32Memory0() {
    if (cachegetInt32Memory0 === null || cachegetInt32Memory0.buffer !== wasm.memory.buffer) {
        cachegetInt32Memory0 = new Int32Array(wasm.memory.buffer);
    }
    return cachegetInt32Memory0;
}

function _assertClass(instance, klass) {
    if (!(instance instanceof klass)) {
        throw new Error(`expected instance of ${klass.name}`);
    }
    return instance.ptr;
}
/**
*/
export function start() {
    wasm.start();
}

function handleError(f) {
    return function () {
        try {
            return f.apply(this, arguments);

        } catch (e) {
            wasm.__wbindgen_exn_store(addHeapObject(e));
        }
    };
}
/**
*/
export const PlayerSide = Object.freeze({ Left:0,"0":"Left",Right:1,"1":"Right", });
/**
*/
export const PlayerCommand = Object.freeze({ Jump:0,"0":"Jump",Left:1,"1":"Left",Right:2,"2":"Right", });
/**
*/
export const CommsChannel = Object.freeze({ ToServerClocksync:0,"0":"ToServerClocksync",ToServerCommand:1,"1":"ToServerCommand",ToClientClocksync:2,"2":"ToClientClocksync",ToClientCommand:3,"3":"ToClientCommand",ToClientSnapshot:4,"4":"ToClientSnapshot", });
/**
*/
export class Demo {

    static __wrap(ptr) {
        const obj = Object.create(Demo.prototype);
        obj.ptr = ptr;

        return obj;
    }

    __destroy_into_raw() {
        const ptr = this.ptr;
        this.ptr = 0;

        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_demo_free(ptr);
    }
    /**
    * @param {number} seconds_since_startup
    */
    constructor(seconds_since_startup) {
        var ret = wasm.demo_new(seconds_since_startup);
        return Demo.__wrap(ret);
    }
    /**
    * @param {number} delta_seconds
    * @param {number} seconds_since_startup
    */
    update(delta_seconds, seconds_since_startup) {
        wasm.demo_update(this.ptr, delta_seconds, seconds_since_startup);
    }
    /**
    * @param {DemoCommand} command
    */
    issue_command(command) {
        _assertClass(command, DemoCommand);
        var ptr0 = command.ptr;
        command.ptr = 0;
        wasm.demo_issue_command(this.ptr, ptr0);
    }
    /**
    * @returns {Array<any>}
    */
    get_server_commands() {
        var ret = wasm.demo_get_server_commands(this.ptr);
        return takeObject(ret);
    }
    /**
    * @param {number} side
    * @returns {Array<any>}
    */
    get_client_commands(side) {
        var ret = wasm.demo_get_client_commands(this.ptr, side);
        return takeObject(ret);
    }
    /**
    * @param {number} side
    * @param {number} channel
    * @returns {number}
    */
    new_comms_activity_count(side, channel) {
        var ret = wasm.demo_new_comms_activity_count(this.ptr, side, channel);
        return ret >>> 0;
    }
    /**
    * @param {number} side
    * @param {number} delay
    */
    set_network_delay(side, delay) {
        wasm.demo_set_network_delay(this.ptr, side, delay);
    }
    /**
    * @param {number} side
    */
    connect(side) {
        wasm.demo_connect(this.ptr, side);
    }
    /**
    * @param {number} side
    */
    disconnect(side) {
        wasm.demo_disconnect(this.ptr, side);
    }
    /**
    * @param {number} side
    * @returns {string}
    */
    client_timestamp(side) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            wasm.demo_client_timestamp(retptr, this.ptr, side);
            var r0 = getInt32Memory0()[retptr / 4 + 0];
            var r1 = getInt32Memory0()[retptr / 4 + 1];
            return getStringFromWasm0(r0, r1);
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
            wasm.__wbindgen_free(r0, r1);
        }
    }
    /**
    * @param {number} side
    * @returns {DemoDisplayState | undefined}
    */
    client_display_state(side) {
        var ret = wasm.demo_client_display_state(this.ptr, side);
        return ret === 0 ? undefined : DemoDisplayState.__wrap(ret);
    }
    /**
    * @param {number} side
    * @returns {string}
    */
    client_reconciliation_status(side) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            wasm.demo_client_reconciliation_status(retptr, this.ptr, side);
            var r0 = getInt32Memory0()[retptr / 4 + 0];
            var r1 = getInt32Memory0()[retptr / 4 + 1];
            return getStringFromWasm0(r0, r1);
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
            wasm.__wbindgen_free(r0, r1);
        }
    }
    /**
    * @returns {string}
    */
    server_timestamp() {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            wasm.demo_server_timestamp(retptr, this.ptr);
            var r0 = getInt32Memory0()[retptr / 4 + 0];
            var r1 = getInt32Memory0()[retptr / 4 + 1];
            return getStringFromWasm0(r0, r1);
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
            wasm.__wbindgen_free(r0, r1);
        }
    }
    /**
    * @returns {DemoDisplayState}
    */
    server_display_state() {
        var ret = wasm.demo_server_display_state(this.ptr);
        return DemoDisplayState.__wrap(ret);
    }
}
/**
*/
export class DemoCommand {

    static __wrap(ptr) {
        const obj = Object.create(DemoCommand.prototype);
        obj.ptr = ptr;

        return obj;
    }

    __destroy_into_raw() {
        const ptr = this.ptr;
        this.ptr = 0;

        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_democommand_free(ptr);
    }
    /**
    * @returns {number}
    */
    get player_side() {
        var ret = wasm.__wbg_get_democommand_player_side(this.ptr);
        return ret >>> 0;
    }
    /**
    * @param {number} arg0
    */
    set player_side(arg0) {
        wasm.__wbg_set_democommand_player_side(this.ptr, arg0);
    }
    /**
    * @returns {number}
    */
    get command() {
        var ret = wasm.__wbg_get_democommand_command(this.ptr);
        return ret >>> 0;
    }
    /**
    * @param {number} arg0
    */
    set command(arg0) {
        wasm.__wbg_set_democommand_command(this.ptr, arg0);
    }
    /**
    * @returns {boolean}
    */
    get value() {
        var ret = wasm.__wbg_get_democommand_value(this.ptr);
        return ret !== 0;
    }
    /**
    * @param {boolean} arg0
    */
    set value(arg0) {
        wasm.__wbg_set_democommand_value(this.ptr, arg0);
    }
    /**
    * @param {number} player_side
    * @param {number} command
    * @param {boolean} value
    */
    constructor(player_side, command, value) {
        var ret = wasm.democommand_new(player_side, command, value);
        return DemoCommand.__wrap(ret);
    }
}
/**
*/
export class DemoDisplayState {

    static __wrap(ptr) {
        const obj = Object.create(DemoDisplayState.prototype);
        obj.ptr = ptr;

        return obj;
    }

    __destroy_into_raw() {
        const ptr = this.ptr;
        this.ptr = 0;

        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_demodisplaystate_free(ptr);
    }
    /**
    * @returns {number}
    */
    player_left_translation_x() {
        var ret = wasm.demodisplaystate_player_left_translation_x(this.ptr);
        return ret;
    }
    /**
    * @returns {number}
    */
    player_left_translation_y() {
        var ret = wasm.demodisplaystate_player_left_translation_y(this.ptr);
        return ret;
    }
    /**
    * @returns {number}
    */
    player_left_angle() {
        var ret = wasm.demodisplaystate_player_left_angle(this.ptr);
        return ret;
    }
    /**
    * @returns {number}
    */
    player_right_translation_x() {
        var ret = wasm.demodisplaystate_player_right_translation_x(this.ptr);
        return ret;
    }
    /**
    * @returns {number}
    */
    player_right_translation_y() {
        var ret = wasm.demodisplaystate_player_right_translation_y(this.ptr);
        return ret;
    }
    /**
    * @returns {number}
    */
    player_right_angle() {
        var ret = wasm.demodisplaystate_player_right_angle(this.ptr);
        return ret;
    }
    /**
    * @returns {number}
    */
    doodad_translation_x() {
        var ret = wasm.demodisplaystate_doodad_translation_x(this.ptr);
        return ret;
    }
    /**
    * @returns {number}
    */
    doodad_translation_y() {
        var ret = wasm.demodisplaystate_doodad_translation_y(this.ptr);
        return ret;
    }
    /**
    * @returns {number}
    */
    doodad_angle() {
        var ret = wasm.demodisplaystate_doodad_angle(this.ptr);
        return ret;
    }
}

async function load(module, imports) {
    if (typeof Response === 'function' && module instanceof Response) {
        if (typeof WebAssembly.instantiateStreaming === 'function') {
            try {
                return await WebAssembly.instantiateStreaming(module, imports);

            } catch (e) {
                if (module.headers.get('Content-Type') != 'application/wasm') {
                    console.warn("`WebAssembly.instantiateStreaming` failed because your server does not serve wasm with `application/wasm` MIME type. Falling back to `WebAssembly.instantiate` which is slower. Original error:\n", e);

                } else {
                    throw e;
                }
            }
        }

        const bytes = await module.arrayBuffer();
        return await WebAssembly.instantiate(bytes, imports);

    } else {
        const instance = await WebAssembly.instantiate(module, imports);

        if (instance instanceof WebAssembly.Instance) {
            return { instance, module };

        } else {
            return instance;
        }
    }
}

async function init(input) {
    if (typeof input === 'undefined') {
        input = new URL('crystalorb_demo_bg.wasm', import.meta.url);
    }
    const imports = {};
    imports.wbg = {};
    imports.wbg.__wbindgen_string_new = function(arg0, arg1) {
        var ret = getStringFromWasm0(arg0, arg1);
        return addHeapObject(ret);
    };
    imports.wbg.__wbindgen_object_drop_ref = function(arg0) {
        takeObject(arg0);
    };
    imports.wbg.__wbg_log_02e20a3c32305fb7 = function(arg0, arg1) {
        try {
            console.log(getStringFromWasm0(arg0, arg1));
        } finally {
            wasm.__wbindgen_free(arg0, arg1);
        }
    };
    imports.wbg.__wbg_log_5c7513aa8c164502 = function(arg0, arg1, arg2, arg3, arg4, arg5, arg6, arg7) {
        try {
            console.log(getStringFromWasm0(arg0, arg1), getStringFromWasm0(arg2, arg3), getStringFromWasm0(arg4, arg5), getStringFromWasm0(arg6, arg7));
        } finally {
            wasm.__wbindgen_free(arg0, arg1);
        }
    };
    imports.wbg.__wbg_mark_abc7631bdced64f0 = function(arg0, arg1) {
        performance.mark(getStringFromWasm0(arg0, arg1));
    };
    imports.wbg.__wbg_measure_c528ff64085b7146 = handleError(function(arg0, arg1, arg2, arg3) {
        try {
            performance.measure(getStringFromWasm0(arg0, arg1), getStringFromWasm0(arg2, arg3));
        } finally {
            wasm.__wbindgen_free(arg0, arg1);
            wasm.__wbindgen_free(arg2, arg3);
        }
    });
    imports.wbg.__wbg_new_693216e109162396 = function() {
        var ret = new Error();
        return addHeapObject(ret);
    };
    imports.wbg.__wbg_stack_0ddaca5d1abfb52f = function(arg0, arg1) {
        var ret = getObject(arg1).stack;
        var ptr0 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        var len0 = WASM_VECTOR_LEN;
        getInt32Memory0()[arg0 / 4 + 1] = len0;
        getInt32Memory0()[arg0 / 4 + 0] = ptr0;
    };
    imports.wbg.__wbg_error_09919627ac0992f5 = function(arg0, arg1) {
        try {
            console.error(getStringFromWasm0(arg0, arg1));
        } finally {
            wasm.__wbindgen_free(arg0, arg1);
        }
    };
    imports.wbg.__wbg_now_bca9396939036a34 = function(arg0) {
        var ret = getObject(arg0).now();
        return ret;
    };
    imports.wbg.__wbg_get_4e90ba4e3de362de = handleError(function(arg0, arg1) {
        var ret = Reflect.get(getObject(arg0), getObject(arg1));
        return addHeapObject(ret);
    });
    imports.wbg.__wbg_call_e5847d15cc228e4f = handleError(function(arg0, arg1) {
        var ret = getObject(arg0).call(getObject(arg1));
        return addHeapObject(ret);
    });
    imports.wbg.__wbindgen_object_clone_ref = function(arg0) {
        var ret = getObject(arg0);
        return addHeapObject(ret);
    };
    imports.wbg.__wbg_new_7c995f2adeba6fb5 = function() {
        var ret = new Array();
        return addHeapObject(ret);
    };
    imports.wbg.__wbg_push_3f7c76b58919ce0d = function(arg0, arg1) {
        var ret = getObject(arg0).push(getObject(arg1));
        return ret;
    };
    imports.wbg.__wbg_newnoargs_2349ba6aefe72376 = function(arg0, arg1) {
        var ret = new Function(getStringFromWasm0(arg0, arg1));
        return addHeapObject(ret);
    };
    imports.wbg.__wbg_self_35a0fda3eb965abe = handleError(function() {
        var ret = self.self;
        return addHeapObject(ret);
    });
    imports.wbg.__wbg_window_88a6f88dd3a474f1 = handleError(function() {
        var ret = window.window;
        return addHeapObject(ret);
    });
    imports.wbg.__wbg_globalThis_1d843c4ad7b6a1f5 = handleError(function() {
        var ret = globalThis.globalThis;
        return addHeapObject(ret);
    });
    imports.wbg.__wbg_global_294ce70448e8fbbf = handleError(function() {
        var ret = global.global;
        return addHeapObject(ret);
    });
    imports.wbg.__wbindgen_is_undefined = function(arg0) {
        var ret = getObject(arg0) === undefined;
        return ret;
    };
    imports.wbg.__wbindgen_debug_string = function(arg0, arg1) {
        var ret = debugString(getObject(arg1));
        var ptr0 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        var len0 = WASM_VECTOR_LEN;
        getInt32Memory0()[arg0 / 4 + 1] = len0;
        getInt32Memory0()[arg0 / 4 + 0] = ptr0;
    };
    imports.wbg.__wbindgen_throw = function(arg0, arg1) {
        throw new Error(getStringFromWasm0(arg0, arg1));
    };

    if (typeof input === 'string' || (typeof Request === 'function' && input instanceof Request) || (typeof URL === 'function' && input instanceof URL)) {
        input = fetch(input);
    }



    const { instance, module } = await load(await input, imports);

    wasm = instance.exports;
    init.__wbindgen_wasm_module = module;
    wasm.__wbindgen_start();
    return wasm;
}

export default init;

