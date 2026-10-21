let wasm_bindgen = (function(exports) {
    let script_src;
    if (typeof document !== 'undefined' && document.currentScript !== null) {
        script_src = new URL(document.currentScript.src, location.href).toString();
    }

    /**
     * Answer a `fetch` event's request. `sw.js` passes the promise to
     * `respondWith`.
     * @param {Request} request
     * @returns {Promise<any>}
     */
    function handle_fetch(request) {
        const ret = wasm.handle_fetch(request);
        return ret;
    }
    exports.handle_fetch = handle_fetch;

    /**
     * Handle a `message` event's data, answering on `port`, the
     * event's first port, when the message asks for an answer. `sw.js`
     * passes the promise to `waitUntil`.
     * @param {any} data
     * @param {any} port
     * @returns {Promise<any>}
     */
    function handle_message(data, port) {
        const ret = wasm.handle_message(data, port);
        return ret;
    }
    exports.handle_message = handle_message;
    function __wbg_get_imports() {
        const import0 = {
            __proto__: null,
            __wbg___wbindgen_bigint_get_as_i64_d968e41184ae354f: function(arg0, arg1) {
                const v = arg1;
                const ret = typeof(v) === 'bigint' ? v : undefined;
                getDataViewMemory0().setBigInt64(arg0 + 8 * 1, isLikeNone(ret) ? BigInt(0) : ret, true);
                getDataViewMemory0().setInt32(arg0 + 4 * 0, !isLikeNone(ret), true);
            },
            __wbg___wbindgen_boolean_get_fa956cfa2d1bd751: function(arg0) {
                const v = arg0;
                const ret = typeof(v) === 'boolean' ? v : undefined;
                return isLikeNone(ret) ? 0xFFFFFF : ret ? 1 : 0;
            },
            __wbg___wbindgen_debug_string_c25d447a39f5578f: function(arg0, arg1) {
                const ret = debugString(arg1);
                const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
                const len1 = WASM_VECTOR_LEN;
                getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
                getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
            },
            __wbg___wbindgen_function_table_808ea415bac79cd6: function() {
                const ret = wasm.__wbindgen_export;
                return ret;
            },
            __wbg___wbindgen_is_function_1ff95bcc5517c252: function(arg0) {
                const ret = typeof(arg0) === 'function';
                return ret;
            },
            __wbg___wbindgen_is_null_ea9085d691f535d3: function(arg0) {
                const ret = arg0 === null;
                return ret;
            },
            __wbg___wbindgen_is_object_a27215656b807791: function(arg0) {
                const val = arg0;
                const ret = typeof(val) === 'object' && val !== null;
                return ret;
            },
            __wbg___wbindgen_is_undefined_c05833b95a3cf397: function(arg0) {
                const ret = arg0 === undefined;
                return ret;
            },
            __wbg___wbindgen_jsval_eq_e659fcf7b0e32763: function(arg0, arg1) {
                const ret = arg0 === arg1;
                return ret;
            },
            __wbg___wbindgen_memory_de265df8aadd6273: function() {
                const ret = wasm.memory;
                return ret;
            },
            __wbg___wbindgen_number_get_394265ed1e1b84ee: function(arg0, arg1) {
                const obj = arg1;
                const ret = typeof(obj) === 'number' ? obj : undefined;
                getDataViewMemory0().setFloat64(arg0 + 8 * 1, isLikeNone(ret) ? 0 : ret, true);
                getDataViewMemory0().setInt32(arg0 + 4 * 0, !isLikeNone(ret), true);
            },
            __wbg___wbindgen_string_get_b0ca35b86a603356: function(arg0, arg1) {
                const obj = arg1;
                const ret = typeof(obj) === 'string' ? obj : undefined;
                var ptr1 = isLikeNone(ret) ? 0 : passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
                var len1 = WASM_VECTOR_LEN;
                getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
                getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
            },
            __wbg___wbindgen_throw_344f42d3211c4765: function(arg0, arg1) {
                throw new Error(getStringFromWasm0(arg0, arg1));
            },
            __wbg__wbg_cb_unref_fffb441def202758: function(arg0) {
                arg0._wbg_cb_unref();
            },
            __wbg_addEventListener_d85450ee1320c989: function() { return handleError(function (arg0, arg1, arg2, arg3) {
                arg0.addEventListener(getStringFromWasm0(arg1, arg2), arg3);
            }, arguments); },
            __wbg_append_01c74e5c6b58aa64: function() { return handleError(function (arg0, arg1, arg2, arg3, arg4) {
                arg0.append(getStringFromWasm0(arg1, arg2), getStringFromWasm0(arg3, arg4));
            }, arguments); },
            __wbg_apply_23dd4d2439189415: function() { return handleError(function (arg0, arg1, arg2) {
                const ret = Reflect.apply(arg0, arg1, arg2);
                return ret;
            }, arguments); },
            __wbg_arrayBuffer_1ae14d3148c4d3eb: function() { return handleError(function (arg0) {
                const ret = arg0.arrayBuffer();
                return ret;
            }, arguments); },
            __wbg_arrayBuffer_3b637f0fa65c5351: function() { return handleError(function (arg0) {
                const ret = arg0.arrayBuffer();
                return ret;
            }, arguments); },
            __wbg_buffer_0f212447ac64c53b: function(arg0) {
                const ret = arg0.buffer;
                return ret;
            },
            __wbg_call_8a2dd23819f8a60a: function() { return handleError(function (arg0, arg1) {
                const ret = arg0.call(arg1);
                return ret;
            }, arguments); },
            __wbg_call_a6e5c5dce5018821: function() { return handleError(function (arg0, arg1, arg2) {
                const ret = arg0.call(arg1, arg2);
                return ret;
            }, arguments); },
            __wbg_call_e3b662382210db98: function() { return handleError(function (arg0, arg1, arg2, arg3) {
                const ret = arg0.call(arg1, arg2, arg3);
                return ret;
            }, arguments); },
            __wbg_clearMarks_14a5d66d895e3bc7: function(arg0) {
                arg0.clearMarks();
            },
            __wbg_clearMeasures_77e51229ba056dd9: function(arg0) {
                arg0.clearMeasures();
            },
            __wbg_clients_e342c77cd8bed164: function(arg0) {
                const ret = arg0.clients;
                return ret;
            },
            __wbg_compile_6372456a133997b2: function(arg0) {
                const ret = WebAssembly.compile(arg0);
                return ret;
            },
            __wbg_construct_4e1a16de27aea5b9: function() { return handleError(function (arg0, arg1) {
                const ret = Reflect.construct(arg0, arg1);
                return ret;
            }, arguments); },
            __wbg_createObjectStore_ff668af6e79f0433: function() { return handleError(function (arg0, arg1, arg2) {
                const ret = arg0.createObjectStore(getStringFromWasm0(arg1, arg2));
                return ret;
            }, arguments); },
            __wbg_create_8dbb796b6159a8e4: function(arg0) {
                const ret = Object.create(arg0);
                return ret;
            },
            __wbg_debug_87fd9b1a625b7efb: function(arg0) {
                console.debug(arg0);
            },
            __wbg_debug_cf53e2612c84e4ee: function(arg0, arg1, arg2, arg3) {
                console.debug(arg0, arg1, arg2, arg3);
            },
            __wbg_done_89b2b13e91a60321: function(arg0) {
                const ret = arg0.done;
                return ret;
            },
            __wbg_error_5b02424faf301d7c: function(arg0) {
                const ret = arg0.error;
                return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
            },
            __wbg_error_657700d53a73881f: function(arg0, arg1, arg2, arg3) {
                console.error(arg0, arg1, arg2, arg3);
            },
            __wbg_error_744744ff0c9861e6: function(arg0) {
                console.error(arg0);
            },
            __wbg_error_becd7e1fe6ce0623: function() { return handleError(function (arg0) {
                const ret = arg0.error;
                return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
            }, arguments); },
            __wbg_exports_085d8a69333b42bc: function(arg0) {
                const ret = arg0.exports;
                return ret;
            },
            __wbg_fetch_b5951fc96f52f786: function(arg0, arg1) {
                const ret = arg0.fetch(arg1);
                return ret;
            },
            __wbg_from_13e323c65fc8f464: function(arg0) {
                const ret = Array.from(arg0);
                return ret;
            },
            __wbg_get_507a50627bffa49b: function(arg0, arg1) {
                const ret = arg0[arg1 >>> 0];
                return ret;
            },
            __wbg_get_53202602c459bfb1: function(arg0, arg1) {
                const ret = arg0.get(arg1);
                return ret;
            },
            __wbg_get_78f252d074a84d0b: function() { return handleError(function (arg0, arg1) {
                const ret = Reflect.get(arg0, arg1);
                return ret;
            }, arguments); },
            __wbg_get_c7eb1f358a7654df: function() { return handleError(function (arg0, arg1) {
                const ret = Reflect.get(arg0, arg1);
                return ret;
            }, arguments); },
            __wbg_get_cefddcaffca4fbb7: function() { return handleError(function (arg0, arg1) {
                const ret = arg0.get(arg1);
                return ret;
            }, arguments); },
            __wbg_get_unchecked_6e0ad6d2a41b06f6: function(arg0, arg1) {
                const ret = arg0[arg1 >>> 0];
                return ret;
            },
            __wbg_grow_921bb12f163cfe6c: function() { return handleError(function (arg0, arg1) {
                const ret = arg0.grow(arg1 >>> 0);
                return ret;
            }, arguments); },
            __wbg_headers_7b59c5203c8c475d: function(arg0) {
                const ret = arg0.headers;
                return ret;
            },
            __wbg_info_79f5309d69d9c70e: function(arg0, arg1, arg2, arg3) {
                console.info(arg0, arg1, arg2, arg3);
            },
            __wbg_info_eadbe775a8e2e9eb: function(arg0) {
                console.info(arg0);
            },
            __wbg_instanceof_Client_fb9be9f30916959d: function(arg0) {
                let result;
                try {
                    result = arg0 instanceof Client;
                } catch (_) {
                    result = false;
                }
                const ret = result;
                return ret;
            },
            __wbg_instanceof_CompileError_7c02db84664b2adb: function(arg0) {
                let result;
                try {
                    result = arg0 instanceof WebAssembly.CompileError;
                } catch (_) {
                    result = false;
                }
                const ret = result;
                return ret;
            },
            __wbg_instanceof_Error_1fdac9f13a8181ba: function(arg0) {
                let result;
                try {
                    result = arg0 instanceof Error;
                } catch (_) {
                    result = false;
                }
                const ret = result;
                return ret;
            },
            __wbg_instanceof_Exception_2560f81d27d99315: function(arg0) {
                let result;
                try {
                    result = arg0 instanceof WebAssembly.Exception;
                } catch (_) {
                    result = false;
                }
                const ret = result;
                return ret;
            },
            __wbg_instanceof_Global_2c2df7a99c3b121b: function(arg0) {
                let result;
                try {
                    result = arg0 instanceof WebAssembly.Global;
                } catch (_) {
                    result = false;
                }
                const ret = result;
                return ret;
            },
            __wbg_instanceof_IdbDatabase_1cc734ba1b040dd7: function(arg0) {
                let result;
                try {
                    result = arg0 instanceof IDBDatabase;
                } catch (_) {
                    result = false;
                }
                const ret = result;
                return ret;
            },
            __wbg_instanceof_IdbFactory_85ccbbe95ef25434: function(arg0) {
                let result;
                try {
                    result = arg0 instanceof IDBFactory;
                } catch (_) {
                    result = false;
                }
                const ret = result;
                return ret;
            },
            __wbg_instanceof_IdbOpenDbRequest_c34a5f3bfadf1d88: function(arg0) {
                let result;
                try {
                    result = arg0 instanceof IDBOpenDBRequest;
                } catch (_) {
                    result = false;
                }
                const ret = result;
                return ret;
            },
            __wbg_instanceof_Instance_d1b5c16044ce60d3: function(arg0) {
                let result;
                try {
                    result = arg0 instanceof WebAssembly.Instance;
                } catch (_) {
                    result = false;
                }
                const ret = result;
                return ret;
            },
            __wbg_instanceof_LinkError_f468fe146203d5ec: function(arg0) {
                let result;
                try {
                    result = arg0 instanceof WebAssembly.LinkError;
                } catch (_) {
                    result = false;
                }
                const ret = result;
                return ret;
            },
            __wbg_instanceof_Memory_2550e651acb3f2f1: function(arg0) {
                let result;
                try {
                    result = arg0 instanceof WebAssembly.Memory;
                } catch (_) {
                    result = false;
                }
                const ret = result;
                return ret;
            },
            __wbg_instanceof_MessagePort_b17bf77c564b57da: function(arg0) {
                let result;
                try {
                    result = arg0 instanceof MessagePort;
                } catch (_) {
                    result = false;
                }
                const ret = result;
                return ret;
            },
            __wbg_instanceof_Module_725d5acf76d9627e: function(arg0) {
                let result;
                try {
                    result = arg0 instanceof WebAssembly.Module;
                } catch (_) {
                    result = false;
                }
                const ret = result;
                return ret;
            },
            __wbg_instanceof_Object_33f20e6f12439f3e: function(arg0) {
                let result;
                try {
                    result = arg0 instanceof Object;
                } catch (_) {
                    result = false;
                }
                const ret = result;
                return ret;
            },
            __wbg_instanceof_Performance_b6ab96b4c12edf1e: function(arg0) {
                let result;
                try {
                    result = arg0 instanceof Performance;
                } catch (_) {
                    result = false;
                }
                const ret = result;
                return ret;
            },
            __wbg_instanceof_Promise_4cb210c0b8f8c959: function(arg0) {
                let result;
                try {
                    result = arg0 instanceof Promise;
                } catch (_) {
                    result = false;
                }
                const ret = result;
                return ret;
            },
            __wbg_instanceof_RangeError_7976f0547ad1279f: function(arg0) {
                let result;
                try {
                    result = arg0 instanceof RangeError;
                } catch (_) {
                    result = false;
                }
                const ret = result;
                return ret;
            },
            __wbg_instanceof_Response_c8b64b2256f01bec: function(arg0) {
                let result;
                try {
                    result = arg0 instanceof Response;
                } catch (_) {
                    result = false;
                }
                const ret = result;
                return ret;
            },
            __wbg_instanceof_RuntimeError_fa0d96398e97d301: function(arg0) {
                let result;
                try {
                    result = arg0 instanceof WebAssembly.RuntimeError;
                } catch (_) {
                    result = false;
                }
                const ret = result;
                return ret;
            },
            __wbg_instanceof_ServiceWorkerGlobalScope_cdda7caab46fb9f0: function(arg0) {
                let result;
                try {
                    result = arg0 instanceof ServiceWorkerGlobalScope;
                } catch (_) {
                    result = false;
                }
                const ret = result;
                return ret;
            },
            __wbg_instanceof_SharedArrayBuffer_23468fe6287a6db3: function(arg0) {
                let result;
                try {
                    result = arg0 instanceof SharedArrayBuffer;
                } catch (_) {
                    result = false;
                }
                const ret = result;
                return ret;
            },
            __wbg_instanceof_Table_4b4d507f81f7ae7c: function(arg0) {
                let result;
                try {
                    result = arg0 instanceof WebAssembly.Table;
                } catch (_) {
                    result = false;
                }
                const ret = result;
                return ret;
            },
            __wbg_instanceof_TypeError_aa4c0c0517d53151: function(arg0) {
                let result;
                try {
                    result = arg0 instanceof TypeError;
                } catch (_) {
                    result = false;
                }
                const ret = result;
                return ret;
            },
            __wbg_isArray_0677c962b281d01a: function(arg0) {
                const ret = Array.isArray(arg0);
                return ret;
            },
            __wbg_is_7b9d0b289033c7de: function(arg0, arg1) {
                const ret = Object.is(arg0, arg1);
                return ret;
            },
            __wbg_iterator_6f722e4a93058b71: function() {
                const ret = Symbol.iterator;
                return ret;
            },
            __wbg_length_1f0964f4a5e2c6d8: function(arg0) {
                const ret = arg0.length;
                return ret;
            },
            __wbg_length_370319915dc99107: function(arg0) {
                const ret = arg0.length;
                return ret;
            },
            __wbg_location_10b37366f445bf01: function(arg0) {
                const ret = arg0.location;
                return ret;
            },
            __wbg_mark_310e9a031ccc84ff: function() { return handleError(function (arg0, arg1, arg2) {
                arg0.mark(getStringFromWasm0(arg1, arg2));
            }, arguments); },
            __wbg_mark_742962d66a858cc5: function() { return handleError(function (arg0, arg1, arg2, arg3) {
                arg0.mark(getStringFromWasm0(arg1, arg2), arg3);
            }, arguments); },
            __wbg_matchAll_1061db1eb945fdb3: function(arg0) {
                const ret = arg0.matchAll();
                return ret;
            },
            __wbg_measure_54743279e77e77bb: function() { return handleError(function (arg0, arg1, arg2, arg3, arg4, arg5, arg6) {
                arg0.measure(getStringFromWasm0(arg1, arg2), getStringFromWasm0(arg3, arg4), getStringFromWasm0(arg5, arg6));
            }, arguments); },
            __wbg_measure_cd1f7667d5f5517d: function() { return handleError(function (arg0, arg1, arg2, arg3) {
                arg0.measure(getStringFromWasm0(arg1, arg2), arg3);
            }, arguments); },
            __wbg_message_8326fb1d549bebc5: function(arg0) {
                const ret = arg0.message;
                return ret;
            },
            __wbg_method_b84982a2d750a05f: function(arg0, arg1) {
                const ret = arg1.method;
                const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
                const len1 = WASM_VECTOR_LEN;
                getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
                getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
            },
            __wbg_new_08cb2fa678b17a48: function() { return handleError(function (arg0, arg1) {
                const ret = new URL(getStringFromWasm0(arg0, arg1));
                return ret;
            }, arguments); },
            __wbg_new_0d809930cd1354c6: function() { return handleError(function () {
                const ret = new Headers();
                return ret;
            }, arguments); },
            __wbg_new_32b398fb48b6d94a: function() {
                const ret = new Array();
                return ret;
            },
            __wbg_new_6eae9a77e2198212: function() { return handleError(function (arg0, arg1) {
                const ret = new WebAssembly.Global(arg0, arg1);
                return ret;
            }, arguments); },
            __wbg_new_711fb31c64c1c363: function() { return handleError(function (arg0) {
                const ret = new WebAssembly.Module(arg0);
                return ret;
            }, arguments); },
            __wbg_new_7796ffc7ed656783: function() {
                const ret = new Map();
                return ret;
            },
            __wbg_new_7837c9d3352daaa0: function() { return handleError(function (arg0) {
                const ret = new WebAssembly.Memory(arg0);
                return ret;
            }, arguments); },
            __wbg_new_944c2b5ed6653041: function() { return handleError(function (arg0, arg1) {
                const ret = new WebAssembly.Instance(arg0, arg1);
                return ret;
            }, arguments); },
            __wbg_new_aec3e25493d729fe: function(arg0, arg1) {
                try {
                    var state0 = {a: arg0, b: arg1};
                    var cb0 = (arg0, arg1) => {
                        const a = state0.a;
                        state0.a = 0;
                        try {
                            return wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___js_sys_6b1da905d9f5533e___Function_fn_wasm_bindgen_7147a55964b576c1___JsValue_____wasm_bindgen_7147a55964b576c1___sys__Undefined___js_sys_6b1da905d9f5533e___Function_fn_wasm_bindgen_7147a55964b576c1___JsValue_____wasm_bindgen_7147a55964b576c1___sys__Undefined_______true_(a, state0.b, arg0, arg1);
                        } finally {
                            state0.a = a;
                        }
                    };
                    const ret = new Promise(cb0);
                    return ret;
                } finally {
                    state0.a = 0;
                }
            },
            __wbg_new_cd45aabdf6073e84: function(arg0) {
                const ret = new Uint8Array(arg0);
                return ret;
            },
            __wbg_new_da52cf8fe3429cb2: function() {
                const ret = new Object();
                return ret;
            },
            __wbg_new_e5e4fd5f6260c7a1: function() { return handleError(function (arg0) {
                const ret = new WebAssembly.Suspending(arg0);
                return ret;
            }, arguments); },
            __wbg_new_from_slice_77cdfb7977362f3c: function(arg0, arg1) {
                const ret = new Uint8Array(getArrayU8FromWasm0(arg0, arg1));
                return ret;
            },
            __wbg_new_typed_1824d93f294193e5: function(arg0, arg1) {
                try {
                    var state0 = {a: arg0, b: arg1};
                    var cb0 = (arg0, arg1) => {
                        const a = state0.a;
                        state0.a = 0;
                        try {
                            return wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___js_sys_6b1da905d9f5533e___Function_fn_wasm_bindgen_7147a55964b576c1___JsValue_____wasm_bindgen_7147a55964b576c1___sys__Undefined___js_sys_6b1da905d9f5533e___Function_fn_wasm_bindgen_7147a55964b576c1___JsValue_____wasm_bindgen_7147a55964b576c1___sys__Undefined_______true_(a, state0.b, arg0, arg1);
                        } finally {
                            state0.a = a;
                        }
                    };
                    const ret = new Promise(cb0);
                    return ret;
                } finally {
                    state0.a = 0;
                }
            },
            __wbg_new_with_byte_offset_and_length_54c7724ee3ec7d82: function(arg0, arg1, arg2) {
                const ret = new Uint8Array(arg0, arg1 >>> 0, arg2 >>> 0);
                return ret;
            },
            __wbg_new_with_opt_str_and_init_74529c37934bd6b4: function() { return handleError(function (arg0, arg1, arg2) {
                const ret = new Response(arg0 === 0 ? undefined : getStringFromWasm0(arg0, arg1), arg2);
                return ret;
            }, arguments); },
            __wbg_new_with_opt_u8_array_and_init_51ce40ffed9fe1b8: function() { return handleError(function (arg0, arg1, arg2) {
                const ret = new Response(arg0 === 0 ? undefined : getArrayU8FromWasm0(arg0, arg1), arg2);
                return ret;
            }, arguments); },
            __wbg_new_with_value_6990dd0a8bc17691: function() { return handleError(function (arg0, arg1) {
                const ret = new WebAssembly.Table(arg0, arg1);
                return ret;
            }, arguments); },
            __wbg_next_6dbf2c0ac8cde20f: function(arg0) {
                const ret = arg0.next;
                return ret;
            },
            __wbg_next_71f2aa1cb3d1e37e: function() { return handleError(function (arg0) {
                const ret = arg0.next();
                return ret;
            }, arguments); },
            __wbg_now_86c0d4ba3fa605b8: function() {
                const ret = Date.now();
                return ret;
            },
            __wbg_objectStore_d5f47956b6c741e3: function() { return handleError(function (arg0, arg1, arg2) {
                const ret = arg0.objectStore(getStringFromWasm0(arg1, arg2));
                return ret;
            }, arguments); },
            __wbg_of_85f52f8b6491a7ca: function(arg0) {
                const ret = Array.of(arg0);
                return ret;
            },
            __wbg_ok_acc5e3fb89668864: function(arg0) {
                const ret = arg0.ok;
                return ret;
            },
            __wbg_open_72e5234a49d5f85d: function() { return handleError(function (arg0, arg1, arg2, arg3) {
                const ret = arg0.open(getStringFromWasm0(arg1, arg2), arg3 >>> 0);
                return ret;
            }, arguments); },
            __wbg_origin_563ae02896bf4edc: function(arg0, arg1) {
                const ret = arg1.origin;
                const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
                const len1 = WASM_VECTOR_LEN;
                getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
                getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
            },
            __wbg_origin_fd7a89859af60f50: function(arg0, arg1) {
                const ret = arg1.origin;
                const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
                const len1 = WASM_VECTOR_LEN;
                getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
                getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
            },
            __wbg_pathname_a701a8de91801a07: function(arg0, arg1) {
                const ret = arg1.pathname;
                const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
                const len1 = WASM_VECTOR_LEN;
                getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
                getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
            },
            __wbg_performance_757310249566272b: function() {
                const ret = globalThis.performance;
                return ret;
            },
            __wbg_postMessage_3d105cccfd888270: function() { return handleError(function (arg0, arg1) {
                arg0.postMessage(arg1);
            }, arguments); },
            __wbg_postMessage_b80f20949a4b4f55: function() { return handleError(function (arg0, arg1) {
                arg0.postMessage(arg1);
            }, arguments); },
            __wbg_prototypesetcall_4770620bbe4688a0: function(arg0, arg1, arg2) {
                Uint8Array.prototype.set.call(getArrayU8FromWasm0(arg0, arg1), arg2);
            },
            __wbg_push_d2ae3af0c1217ae6: function(arg0, arg1) {
                const ret = arg0.push(arg1);
                return ret;
            },
            __wbg_put_a368805e3dcab3a7: function() { return handleError(function (arg0, arg1, arg2) {
                const ret = arg0.put(arg1, arg2);
                return ret;
            }, arguments); },
            __wbg_queueMicrotask_0ab5b2d2393e99b9: function(arg0) {
                const ret = arg0.queueMicrotask;
                return ret;
            },
            __wbg_queueMicrotask_6a09b7bc46549209: function(arg0) {
                queueMicrotask(arg0);
            },
            __wbg_reject_90df95b492dd8563: function(arg0) {
                const ret = Promise.reject(arg0);
                return ret;
            },
            __wbg_removeEventListener_a3f23c70077bdcc1: function() { return handleError(function (arg0, arg1, arg2, arg3) {
                arg0.removeEventListener(getStringFromWasm0(arg1, arg2), arg3);
            }, arguments); },
            __wbg_resolve_2191a4dfe481c25b: function(arg0) {
                const ret = Promise.resolve(arg0);
                return ret;
            },
            __wbg_result_2b1294a2bf8dc773: function() { return handleError(function (arg0) {
                const ret = arg0.result;
                return ret;
            }, arguments); },
            __wbg_search_c905fb82fd20bc6b: function(arg0, arg1) {
                const ret = arg1.search;
                const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
                const len1 = WASM_VECTOR_LEN;
                getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
                getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
            },
            __wbg_set_4d7dd76f3dae2926: function(arg0, arg1, arg2) {
                arg0.set(getArrayU8FromWasm0(arg1, arg2));
            },
            __wbg_set_575dd786d51585f8: function(arg0, arg1, arg2) {
                const ret = arg0.set(arg1, arg2);
                return ret;
            },
            __wbg_set_5d8eaa6b2caf4444: function() { return handleError(function (arg0, arg1, arg2) {
                arg0.set(arg1 >>> 0, arg2);
            }, arguments); },
            __wbg_set_61e45ae8061eca11: function(arg0, arg1, arg2) {
                arg0.set(arg1, arg2 >>> 0);
            },
            __wbg_set_8535240470bf2500: function() { return handleError(function (arg0, arg1, arg2) {
                const ret = Reflect.set(arg0, arg1, arg2);
                return ret;
            }, arguments); },
            __wbg_set_headers_4be66d6f175ce615: function(arg0, arg1) {
                arg0.headers = arg1;
            },
            __wbg_set_onupgradeneeded_7b2cf4ba1c57e655: function(arg0, arg1) {
                arg0.onupgradeneeded = arg1;
            },
            __wbg_set_status_3593dfcb55e7ee3c: function(arg0, arg1) {
                arg0.status = arg1;
            },
            __wbg_set_value_f86d819107cb271b: function(arg0, arg1) {
                arg0.value = arg1;
            },
            __wbg_slice_2b88ff0ac64039d6: function(arg0, arg1) {
                const ret = arg1.slice();
                const ptr1 = passArrayJsValueToWasm0(ret, wasm.__wbindgen_malloc);
                const len1 = WASM_VECTOR_LEN;
                getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
                getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
            },
            __wbg_static_accessor_GLOBAL_4ef717fb391d88b7: function() {
                const ret = typeof global === 'undefined' ? null : global;
                return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
            },
            __wbg_static_accessor_GLOBAL_THIS_8d1badc68b5a74f4: function() {
                const ret = typeof globalThis === 'undefined' ? null : globalThis;
                return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
            },
            __wbg_static_accessor_SELF_146583524fe1469b: function() {
                const ret = typeof self === 'undefined' ? null : self;
                return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
            },
            __wbg_static_accessor_WINDOW_f2829a2234d7819e: function() {
                const ret = typeof window === 'undefined' ? null : window;
                return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
            },
            __wbg_status_c45b3b9b3033184a: function(arg0) {
                const ret = arg0.status;
                return ret;
            },
            __wbg_target_e759594a8d965ed7: function(arg0) {
                const ret = arg0.target;
                return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
            },
            __wbg_then_16d107c451e9905d: function(arg0, arg1, arg2) {
                const ret = arg0.then(arg1, arg2);
                return ret;
            },
            __wbg_then_6ec10ae38b3e92f7: function(arg0, arg1) {
                const ret = arg0.then(arg1);
                return ret;
            },
            __wbg_transaction_d911d96b4b0af154: function() { return handleError(function (arg0, arg1, arg2, arg3) {
                const ret = arg0.transaction(getStringFromWasm0(arg1, arg2), __wbindgen_enum_IdbTransactionMode[arg3]);
                return ret;
            }, arguments); },
            __wbg_url_f6cd241d61f89b82: function(arg0, arg1) {
                const ret = arg1.url;
                const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
                const len1 = WASM_VECTOR_LEN;
                getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
                getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
            },
            __wbg_validate_2bf76e0e61374b8e: function() { return handleError(function (arg0) {
                const ret = WebAssembly.validate(arg0);
                return ret;
            }, arguments); },
            __wbg_value_5bf3641a3c74fb47: function(arg0) {
                const ret = arg0.value;
                return ret;
            },
            __wbg_value_a5d5488a9589444a: function(arg0) {
                const ret = arg0.value;
                return ret;
            },
            __wbg_warn_77c4eb4a21e10a21: function(arg0, arg1, arg2, arg3) {
                console.warn(arg0, arg1, arg2, arg3);
            },
            __wbg_warn_b1370d804fa3e259: function(arg0) {
                console.warn(arg0);
            },
            __wbindgen_cast_0000000000000001: function(arg0, arg1) {
                // Cast intrinsic for `Closure(Closure { owned: true, function: Function { arguments: [Externref], shim_idx: 4664, ret: Result(Unit), inner_ret: Some(Result(Unit)) }, mutable: true }) -> Externref`.
                const ret = makeMutClosure(arg0, arg1, wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___wasm_bindgen_7147a55964b576c1___JsValue__core_37ee0ce0c4cd57d5___result__Result_____wasm_bindgen_7147a55964b576c1___JsError___true_);
                return ret;
            },
            __wbindgen_cast_0000000000000002: function(arg0, arg1) {
                // Cast intrinsic for `Closure(Closure { owned: true, function: Function { arguments: [Externref], shim_idx: 861, ret: Result(Unit), inner_ret: Some(Result(Unit)) }, mutable: false }) -> Externref`.
                const ret = makeClosure(arg0, arg1, wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___wasm_bindgen_7147a55964b576c1___JsValue__core_37ee0ce0c4cd57d5___result__Result_____wasm_bindgen_7147a55964b576c1___JsValue___true_);
                return ret;
            },
            __wbindgen_cast_0000000000000003: function(arg0, arg1) {
                // Cast intrinsic for `Closure(Closure { owned: true, function: Function { arguments: [Externref], shim_idx: 879, ret: Unit, inner_ret: Some(Unit) }, mutable: true }) -> Externref`.
                const ret = makeMutClosure(arg0, arg1, wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___wasm_bindgen_7147a55964b576c1___JsValue______true_);
                return ret;
            },
            __wbindgen_cast_0000000000000004: function(arg0, arg1) {
                // Cast intrinsic for `Closure(Closure { owned: true, function: Function { arguments: [NamedExternref("Event")], shim_idx: 837, ret: Unit, inner_ret: Some(Unit) }, mutable: true }) -> Externref`.
                const ret = makeMutClosure(arg0, arg1, wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___web_sys_e24fb17a5cad9f88___features__gen_MessageEvent__MessageEvent______true_);
                return ret;
            },
            __wbindgen_cast_0000000000000005: function(arg0, arg1) {
                // Cast intrinsic for `Closure(Closure { owned: true, function: Function { arguments: [U32, Externref, Externref, Externref, Externref], shim_idx: 1096, ret: I32, inner_ret: Some(I32) }, mutable: false }) -> Externref`.
                const ret = makeClosure(arg0, arg1, wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___u32__wasm_bindgen_7147a55964b576c1___JsValue__wasm_bindgen_7147a55964b576c1___JsValue__wasm_bindgen_7147a55964b576c1___JsValue__wasm_bindgen_7147a55964b576c1___JsValue__i32__true_);
                return ret;
            },
            __wbindgen_cast_0000000000000006: function(arg0, arg1) {
                // Cast intrinsic for `Closure(Closure { owned: true, function: Function { arguments: [U32, Externref, Externref, Externref], shim_idx: 870, ret: I32, inner_ret: Some(I32) }, mutable: false }) -> Externref`.
                const ret = makeClosure(arg0, arg1, wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___u32__wasm_bindgen_7147a55964b576c1___JsValue__wasm_bindgen_7147a55964b576c1___JsValue__wasm_bindgen_7147a55964b576c1___JsValue__i32__true_);
                return ret;
            },
            __wbindgen_cast_0000000000000007: function(arg0, arg1) {
                // Cast intrinsic for `Closure(Closure { owned: true, function: Function { arguments: [U32, Externref, Externref], shim_idx: 872, ret: I32, inner_ret: Some(I32) }, mutable: false }) -> Externref`.
                const ret = makeClosure(arg0, arg1, wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___u32__wasm_bindgen_7147a55964b576c1___JsValue__wasm_bindgen_7147a55964b576c1___JsValue__i32__true_);
                return ret;
            },
            __wbindgen_cast_0000000000000008: function(arg0, arg1) {
                // Cast intrinsic for `Closure(Closure { owned: true, function: Function { arguments: [U32, Externref], shim_idx: 874, ret: I32, inner_ret: Some(I32) }, mutable: false }) -> Externref`.
                const ret = makeClosure(arg0, arg1, wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___u32__wasm_bindgen_7147a55964b576c1___JsValue__i32__true_);
                return ret;
            },
            __wbindgen_cast_0000000000000009: function(arg0, arg1) {
                // Cast intrinsic for `Closure(Closure { owned: true, function: Function { arguments: [U32, Externref], shim_idx: 876, ret: Unit, inner_ret: Some(Unit) }, mutable: false }) -> Externref`.
                const ret = makeClosure(arg0, arg1, wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___u32__wasm_bindgen_7147a55964b576c1___JsValue______true_);
                return ret;
            },
            __wbindgen_cast_000000000000000a: function(arg0, arg1) {
                // Cast intrinsic for `Closure(Closure { owned: true, function: Function { arguments: [U32, U32], shim_idx: 874, ret: Externref, inner_ret: Some(Externref) }, mutable: false }) -> Externref`.
                const ret = makeClosure(arg0, arg1, wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___u32__wasm_bindgen_7147a55964b576c1___JsValue__i32__true__9);
                return ret;
            },
            __wbindgen_cast_000000000000000b: function(arg0, arg1) {
                // Cast intrinsic for `Closure(Closure { owned: true, function: Function { arguments: [U32], shim_idx: 863, ret: Externref, inner_ret: Some(Externref) }, mutable: false }) -> Externref`.
                const ret = makeClosure(arg0, arg1, wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___u32__wasm_bindgen_7147a55964b576c1___JsValue__true_);
                return ret;
            },
            __wbindgen_cast_000000000000000c: function(arg0, arg1) {
                // Cast intrinsic for `Closure(Closure { owned: true, function: Function { arguments: [U32], shim_idx: 863, ret: I32, inner_ret: Some(I32) }, mutable: false }) -> Externref`.
                const ret = makeClosure(arg0, arg1, wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___u32__wasm_bindgen_7147a55964b576c1___JsValue__true__11);
                return ret;
            },
            __wbindgen_cast_000000000000000d: function(arg0, arg1) {
                // Cast intrinsic for `Closure(Closure { owned: true, function: Function { arguments: [U32], shim_idx: 863, ret: U32, inner_ret: Some(U32) }, mutable: false }) -> Externref`.
                const ret = makeClosure(arg0, arg1, wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___u32__wasm_bindgen_7147a55964b576c1___JsValue__true__12);
                return ret;
            },
            __wbindgen_cast_000000000000000e: function(arg0, arg1) {
                // Cast intrinsic for `Closure(Closure { owned: true, function: Function { arguments: [U32], shim_idx: 867, ret: Unit, inner_ret: Some(Unit) }, mutable: false }) -> Externref`.
                const ret = makeClosure(arg0, arg1, wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___u32______true_);
                return ret;
            },
            __wbindgen_cast_000000000000000f: function(arg0, arg1) {
                // Cast intrinsic for `Closure(Closure { owned: true, function: Function { arguments: [], shim_idx: 1449, ret: Unit, inner_ret: Some(Unit) }, mutable: true }) -> Externref`.
                const ret = makeMutClosure(arg0, arg1, wasm_bindgen_7147a55964b576c1___convert__closures_____invoke_______true__2_);
                return ret;
            },
            __wbindgen_cast_0000000000000010: function(arg0, arg1) {
                // Cast intrinsic for `Closure(Closure { owned: true, function: Function { arguments: [], shim_idx: 835, ret: Unit, inner_ret: Some(Unit) }, mutable: false }) -> Externref`.
                const ret = makeClosure(arg0, arg1, wasm_bindgen_7147a55964b576c1___convert__closures_____invoke_______true_);
                return ret;
            },
            __wbindgen_cast_0000000000000011: function(arg0, arg1) {
                // Cast intrinsic for `Closure(Closure { owned: true, function: Function { arguments: [], shim_idx: 881, ret: Unit, inner_ret: Some(Unit) }, mutable: true }) -> Externref`.
                const ret = makeMutClosure(arg0, arg1, wasm_bindgen_7147a55964b576c1___convert__closures_____invoke_______true__1_);
                return ret;
            },
            __wbindgen_cast_0000000000000012: function(arg0) {
                // Cast intrinsic for `F64 -> Externref`.
                const ret = arg0;
                return ret;
            },
            __wbindgen_cast_0000000000000013: function(arg0) {
                // Cast intrinsic for `I64 -> Externref`.
                const ret = arg0;
                return ret;
            },
            __wbindgen_cast_0000000000000014: function(arg0, arg1) {
                // Cast intrinsic for `Ref(String) -> Externref`.
                const ret = getStringFromWasm0(arg0, arg1);
                return ret;
            },
            __wbindgen_cast_0000000000000015: function(arg0) {
                // Cast intrinsic for `U64 -> Externref`.
                const ret = BigInt.asUintN(64, arg0);
                return ret;
            },
            __wbindgen_init_externref_table: function() {
                const table = wasm.__wbindgen_externrefs;
                const offset = table.grow(4);
                table.set(0, undefined);
                table.set(offset + 0, undefined);
                table.set(offset + 1, null);
                table.set(offset + 2, true);
                table.set(offset + 3, false);
            },
        };
        return {
            __proto__: null,
            "./conduct-demo-worker_bg.js": import0,
        };
    }

    function wasm_bindgen_7147a55964b576c1___convert__closures_____invoke_______true__2_(arg0, arg1) {
        wasm.wasm_bindgen_7147a55964b576c1___convert__closures_____invoke_______true__2_(arg0, arg1);
    }

    function wasm_bindgen_7147a55964b576c1___convert__closures_____invoke_______true_(arg0, arg1) {
        wasm.wasm_bindgen_7147a55964b576c1___convert__closures_____invoke_______true_(arg0, arg1);
    }

    function wasm_bindgen_7147a55964b576c1___convert__closures_____invoke_______true__1_(arg0, arg1) {
        wasm.wasm_bindgen_7147a55964b576c1___convert__closures_____invoke_______true__1_(arg0, arg1);
    }

    function wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___wasm_bindgen_7147a55964b576c1___JsValue______true_(arg0, arg1, arg2) {
        wasm.wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___wasm_bindgen_7147a55964b576c1___JsValue______true_(arg0, arg1, arg2);
    }

    function wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___web_sys_e24fb17a5cad9f88___features__gen_MessageEvent__MessageEvent______true_(arg0, arg1, arg2) {
        wasm.wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___web_sys_e24fb17a5cad9f88___features__gen_MessageEvent__MessageEvent______true_(arg0, arg1, arg2);
    }

    function wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___wasm_bindgen_7147a55964b576c1___JsValue__core_37ee0ce0c4cd57d5___result__Result_____wasm_bindgen_7147a55964b576c1___JsError___true_(arg0, arg1, arg2) {
        const ret = wasm.wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___wasm_bindgen_7147a55964b576c1___JsValue__core_37ee0ce0c4cd57d5___result__Result_____wasm_bindgen_7147a55964b576c1___JsError___true_(arg0, arg1, arg2);
        if (ret[1]) {
            throw takeFromExternrefTable0(ret[0]);
        }
    }

    function wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___wasm_bindgen_7147a55964b576c1___JsValue__core_37ee0ce0c4cd57d5___result__Result_____wasm_bindgen_7147a55964b576c1___JsValue___true_(arg0, arg1, arg2) {
        const ret = wasm.wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___wasm_bindgen_7147a55964b576c1___JsValue__core_37ee0ce0c4cd57d5___result__Result_____wasm_bindgen_7147a55964b576c1___JsValue___true_(arg0, arg1, arg2);
        if (ret[1]) {
            throw takeFromExternrefTable0(ret[0]);
        }
    }

    function wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___js_sys_6b1da905d9f5533e___Function_fn_wasm_bindgen_7147a55964b576c1___JsValue_____wasm_bindgen_7147a55964b576c1___sys__Undefined___js_sys_6b1da905d9f5533e___Function_fn_wasm_bindgen_7147a55964b576c1___JsValue_____wasm_bindgen_7147a55964b576c1___sys__Undefined_______true_(arg0, arg1, arg2, arg3) {
        wasm.wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___js_sys_6b1da905d9f5533e___Function_fn_wasm_bindgen_7147a55964b576c1___JsValue_____wasm_bindgen_7147a55964b576c1___sys__Undefined___js_sys_6b1da905d9f5533e___Function_fn_wasm_bindgen_7147a55964b576c1___JsValue_____wasm_bindgen_7147a55964b576c1___sys__Undefined_______true_(arg0, arg1, arg2, arg3);
    }

    function wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___u32______true_(arg0, arg1, arg2) {
        wasm.wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___u32______true_(arg0, arg1, arg2);
    }

    function wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___u32__wasm_bindgen_7147a55964b576c1___JsValue__true_(arg0, arg1, arg2) {
        const ret = wasm.wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___u32__wasm_bindgen_7147a55964b576c1___JsValue__true_(arg0, arg1, arg2);
        return ret;
    }

    function wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___u32__wasm_bindgen_7147a55964b576c1___JsValue__true__11(arg0, arg1, arg2) {
        const ret = wasm.wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___u32__wasm_bindgen_7147a55964b576c1___JsValue__true__11(arg0, arg1, arg2);
        return ret;
    }

    function wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___u32__wasm_bindgen_7147a55964b576c1___JsValue__true__12(arg0, arg1, arg2) {
        const ret = wasm.wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___u32__wasm_bindgen_7147a55964b576c1___JsValue__true__12(arg0, arg1, arg2);
        return ret >>> 0;
    }

    function wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___u32__wasm_bindgen_7147a55964b576c1___JsValue______true_(arg0, arg1, arg2, arg3) {
        wasm.wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___u32__wasm_bindgen_7147a55964b576c1___JsValue______true_(arg0, arg1, arg2, arg3);
    }

    function wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___u32__wasm_bindgen_7147a55964b576c1___JsValue__i32__true_(arg0, arg1, arg2, arg3) {
        const ret = wasm.wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___u32__wasm_bindgen_7147a55964b576c1___JsValue__i32__true_(arg0, arg1, arg2, arg3);
        return ret;
    }

    function wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___u32__wasm_bindgen_7147a55964b576c1___JsValue__wasm_bindgen_7147a55964b576c1___JsValue__i32__true_(arg0, arg1, arg2, arg3, arg4) {
        const ret = wasm.wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___u32__wasm_bindgen_7147a55964b576c1___JsValue__wasm_bindgen_7147a55964b576c1___JsValue__i32__true_(arg0, arg1, arg2, arg3, arg4);
        return ret;
    }

    function wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___u32__wasm_bindgen_7147a55964b576c1___JsValue__wasm_bindgen_7147a55964b576c1___JsValue__wasm_bindgen_7147a55964b576c1___JsValue__i32__true_(arg0, arg1, arg2, arg3, arg4, arg5) {
        const ret = wasm.wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___u32__wasm_bindgen_7147a55964b576c1___JsValue__wasm_bindgen_7147a55964b576c1___JsValue__wasm_bindgen_7147a55964b576c1___JsValue__i32__true_(arg0, arg1, arg2, arg3, arg4, arg5);
        return ret;
    }

    function wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___u32__wasm_bindgen_7147a55964b576c1___JsValue__wasm_bindgen_7147a55964b576c1___JsValue__wasm_bindgen_7147a55964b576c1___JsValue__wasm_bindgen_7147a55964b576c1___JsValue__i32__true_(arg0, arg1, arg2, arg3, arg4, arg5, arg6) {
        const ret = wasm.wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___u32__wasm_bindgen_7147a55964b576c1___JsValue__wasm_bindgen_7147a55964b576c1___JsValue__wasm_bindgen_7147a55964b576c1___JsValue__wasm_bindgen_7147a55964b576c1___JsValue__i32__true_(arg0, arg1, arg2, arg3, arg4, arg5, arg6);
        return ret;
    }

    function wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___u32__wasm_bindgen_7147a55964b576c1___JsValue__i32__true__9(arg0, arg1, arg2, arg3) {
        const ret = wasm.wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___u32__wasm_bindgen_7147a55964b576c1___JsValue__i32__true__9(arg0, arg1, arg2, arg3);
        return ret;
    }


    const __wbindgen_enum_IdbTransactionMode = ["readonly", "readwrite", "versionchange", "readwriteflush", "cleanup"];

    function addToExternrefTable0(obj) {
        const idx = wasm.__externref_table_alloc();
        wasm.__wbindgen_externrefs.set(idx, obj);
        return idx;
    }

    const CLOSURE_DTORS = (typeof FinalizationRegistry === 'undefined')
        ? { register: () => {}, unregister: () => {} }
        : new FinalizationRegistry(state => wasm.__wbindgen_destroy_closure(state.a, state.b));

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
        if (builtInMatches && builtInMatches.length > 1) {
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

    function getArrayU8FromWasm0(ptr, len) {
        ptr = ptr >>> 0;
        return getUint8ArrayMemory0().subarray(ptr / 1, ptr / 1 + len);
    }

    let cachedDataViewMemory0 = null;
    function getDataViewMemory0() {
        if (cachedDataViewMemory0 === null || cachedDataViewMemory0.buffer.detached === true || (cachedDataViewMemory0.buffer.detached === undefined && cachedDataViewMemory0.buffer !== wasm.memory.buffer)) {
            cachedDataViewMemory0 = new DataView(wasm.memory.buffer);
        }
        return cachedDataViewMemory0;
    }

    function getStringFromWasm0(ptr, len) {
        return decodeText(ptr >>> 0, len);
    }

    let cachedUint8ArrayMemory0 = null;
    function getUint8ArrayMemory0() {
        if (cachedUint8ArrayMemory0 === null || cachedUint8ArrayMemory0.byteLength === 0) {
            cachedUint8ArrayMemory0 = new Uint8Array(wasm.memory.buffer);
        }
        return cachedUint8ArrayMemory0;
    }

    function handleError(f, args) {
        try {
            return f.apply(this, args);
        } catch (e) {
            const idx = addToExternrefTable0(e);
            wasm.__wbindgen_exn_store(idx);
        }
    }

    function isLikeNone(x) {
        return x === undefined || x === null;
    }

    function makeClosure(arg0, arg1, f) {
        const state = { a: arg0, b: arg1, cnt: 1 };
        const real = (...args) => {

            // First up with a closure we increment the internal reference
            // count. This ensures that the Rust closure environment won't
            // be deallocated while we're invoking it.
            state.cnt++;
            try {
                return f(state.a, state.b, ...args);
            } finally {
                real._wbg_cb_unref();
            }
        };
        real._wbg_cb_unref = () => {
            if (--state.cnt === 0) {
                wasm.__wbindgen_destroy_closure(state.a, state.b);
                state.a = 0;
                CLOSURE_DTORS.unregister(state);
            }
        };
        CLOSURE_DTORS.register(real, state, state);
        return real;
    }

    function makeMutClosure(arg0, arg1, f) {
        const state = { a: arg0, b: arg1, cnt: 1 };
        const real = (...args) => {

            // First up with a closure we increment the internal reference
            // count. This ensures that the Rust closure environment won't
            // be deallocated while we're invoking it.
            state.cnt++;
            const a = state.a;
            state.a = 0;
            try {
                return f(a, state.b, ...args);
            } finally {
                state.a = a;
                real._wbg_cb_unref();
            }
        };
        real._wbg_cb_unref = () => {
            if (--state.cnt === 0) {
                wasm.__wbindgen_destroy_closure(state.a, state.b);
                state.a = 0;
                CLOSURE_DTORS.unregister(state);
            }
        };
        CLOSURE_DTORS.register(real, state, state);
        return real;
    }

    function passArrayJsValueToWasm0(array, malloc) {
        const ptr = malloc(array.length * 4, 4) >>> 0;
        for (let i = 0; i < array.length; i++) {
            const add = addToExternrefTable0(array[i]);
            getDataViewMemory0().setUint32(ptr + 4 * i, add, true);
        }
        WASM_VECTOR_LEN = array.length;
        return ptr;
    }

    function passStringToWasm0(arg, malloc, realloc) {
        if (realloc === undefined) {
            const buf = cachedTextEncoder.encode(arg);
            const ptr = malloc(buf.length, 1) >>> 0;
            getUint8ArrayMemory0().subarray(ptr, ptr + buf.length).set(buf);
            WASM_VECTOR_LEN = buf.length;
            return ptr;
        }

        let len = arg.length;
        let ptr = malloc(len, 1) >>> 0;

        const mem = getUint8ArrayMemory0();

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
            ptr = realloc(ptr, len, len = offset + arg.length * 3, 1) >>> 0;
            const view = getUint8ArrayMemory0().subarray(ptr + offset, ptr + len);
            const ret = cachedTextEncoder.encodeInto(arg, view);

            offset += ret.written;
            ptr = realloc(ptr, len, offset, 1) >>> 0;
        }

        WASM_VECTOR_LEN = offset;
        return ptr;
    }

    function takeFromExternrefTable0(idx) {
        const value = wasm.__wbindgen_externrefs.get(idx);
        wasm.__externref_table_dealloc(idx);
        return value;
    }

    let cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true, fatal: true });
    cachedTextDecoder.decode();
    function decodeText(ptr, len) {
        return cachedTextDecoder.decode(getUint8ArrayMemory0().subarray(ptr, ptr + len));
    }

    const cachedTextEncoder = new TextEncoder();

    if (!('encodeInto' in cachedTextEncoder)) {
        cachedTextEncoder.encodeInto = function (arg, view) {
            const buf = cachedTextEncoder.encode(arg);
            view.set(buf);
            return {
                read: arg.length,
                written: buf.length
            };
        };
    }

    let WASM_VECTOR_LEN = 0;

    let wasmModule, wasmInstance, wasm;
    function __wbg_finalize_init(instance, module) {
        wasmInstance = instance;
        wasm = instance.exports;
        wasmModule = module;
        cachedDataViewMemory0 = null;
        cachedUint8ArrayMemory0 = null;
        wasm.__wbindgen_start();
        return wasm;
    }

    async function __wbg_load(module, imports) {
        if (typeof Response === 'function' && module instanceof Response) {
            if (typeof WebAssembly.instantiateStreaming === 'function') {
                try {
                    return await WebAssembly.instantiateStreaming(module, imports);
                } catch (e) {
                    const validResponse = module.ok && expectedResponseType(module.type);

                    if (validResponse && module.headers.get('Content-Type') !== 'application/wasm') {
                        console.warn("`WebAssembly.instantiateStreaming` failed because your server does not serve Wasm with `application/wasm` MIME type. Falling back to `WebAssembly.instantiate` which is slower. Original error:\n", e);

                    } else { throw e; }
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

        function expectedResponseType(type) {
            switch (type) {
                case 'basic': case 'cors': case 'default': return true;
            }
            return false;
        }
    }

    function initSync(module) {
        if (wasm !== undefined) return wasm;


        if (module !== undefined) {
            if (Object.getPrototypeOf(module) === Object.prototype) {
                ({module} = module)
            } else {
                console.warn('using deprecated parameters for `initSync()`; pass a single object instead')
            }
        }

        const imports = __wbg_get_imports();
        if (!(module instanceof WebAssembly.Module)) {
            module = new WebAssembly.Module(module);
        }
        const instance = new WebAssembly.Instance(module, imports);
        return __wbg_finalize_init(instance, module);
    }

    async function __wbg_init(module_or_path) {
        if (wasm !== undefined) return wasm;


        if (module_or_path !== undefined) {
            if (Object.getPrototypeOf(module_or_path) === Object.prototype) {
                ({module_or_path} = module_or_path)
            } else {
                console.warn('using deprecated parameters for the initialization function; pass a single object instead')
            }
        }

        if (module_or_path === undefined && script_src !== undefined) {
            module_or_path = script_src.replace(/\.js$/, "_bg.wasm");
        }
        const imports = __wbg_get_imports();

        if (typeof module_or_path === 'string' || (typeof Request === 'function' && module_or_path instanceof Request) || (typeof URL === 'function' && module_or_path instanceof URL)) {
            module_or_path = fetch(module_or_path);
        }

        const { instance, module } = await __wbg_load(await module_or_path, imports);

        return __wbg_finalize_init(instance, module);
    }

    return Object.assign(__wbg_init, { initSync }, exports);
})({ __proto__: null });
