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
        __wbg___wbindgen_is_falsy_a6dfe792ff282f10: function(arg0) {
            const ret = !arg0;
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
        __wbg_active_d5ba5c341fd5b1d9: function(arg0) {
            const ret = arg0.active;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        },
        __wbg_addEventListener_109ae44e5cc4d506: function() { return handleError(function (arg0, arg1, arg2, arg3, arg4) {
            arg0.addEventListener(getStringFromWasm0(arg1, arg2), arg3, arg4 !== 0);
        }, arguments); },
        __wbg_addEventListener_520e749bbae24529: function() { return handleError(function (arg0, arg1, arg2, arg3, arg4) {
            arg0.addEventListener(getStringFromWasm0(arg1, arg2), arg3, arg4);
        }, arguments); },
        __wbg_addEventListener_d85450ee1320c989: function() { return handleError(function (arg0, arg1, arg2, arg3) {
            arg0.addEventListener(getStringFromWasm0(arg1, arg2), arg3);
        }, arguments); },
        __wbg_altKey_50f830d1793a2eea: function(arg0) {
            const ret = arg0.altKey;
            return ret;
        },
        __wbg_appendChild_f553e8704c4f14a6: function() { return handleError(function (arg0, arg1) {
            const ret = arg0.appendChild(arg1);
            return ret;
        }, arguments); },
        __wbg_append_01c74e5c6b58aa64: function() { return handleError(function (arg0, arg1, arg2, arg3, arg4) {
            arg0.append(getStringFromWasm0(arg1, arg2), getStringFromWasm0(arg3, arg4));
        }, arguments); },
        __wbg_apply_23dd4d2439189415: function() { return handleError(function (arg0, arg1, arg2) {
            const ret = Reflect.apply(arg0, arg1, arg2);
            return ret;
        }, arguments); },
        __wbg_arrayBuffer_3b637f0fa65c5351: function() { return handleError(function (arg0) {
            const ret = arg0.arrayBuffer();
            return ret;
        }, arguments); },
        __wbg_attributes_d56d426c1a872540: function(arg0) {
            const ret = arg0.attributes;
            return ret;
        },
        __wbg_blockSize_5af477b962b2b031: function(arg0) {
            const ret = arg0.blockSize;
            return ret;
        },
        __wbg_blur_e902dcc79406e89c: function() { return handleError(function (arg0) {
            arg0.blur();
        }, arguments); },
        __wbg_body_40ec34e0a2931fe8: function(arg0) {
            const ret = arg0.body;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        },
        __wbg_borderBoxSize_ff7f5405dcc6554e: function(arg0) {
            const ret = arg0.borderBoxSize;
            return ret;
        },
        __wbg_buffer_0f212447ac64c53b: function(arg0) {
            const ret = arg0.buffer;
            return ret;
        },
        __wbg_call_44b7209e1e252e6a: function() { return handleError(function (arg0, arg1, arg2, arg3, arg4) {
            const ret = arg0.call(arg1, arg2, arg3, arg4);
            return ret;
        }, arguments); },
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
        __wbg_children_88145a67dc87f2ad: function(arg0) {
            const ret = arg0.children;
            return ret;
        },
        __wbg_clearMarks_14a5d66d895e3bc7: function(arg0) {
            arg0.clearMarks();
        },
        __wbg_clearMeasures_77e51229ba056dd9: function(arg0) {
            arg0.clearMeasures();
        },
        __wbg_clearTimeout_8f80437be2324e09: function(arg0, arg1) {
            arg0.clearTimeout(arg1);
        },
        __wbg_clientX_a7dcb4081126cd4b: function(arg0) {
            const ret = arg0.clientX;
            return ret;
        },
        __wbg_clientY_c0560910b20ee192: function(arg0) {
            const ret = arg0.clientY;
            return ret;
        },
        __wbg_closest_d889c758da4bb13b: function() { return handleError(function (arg0, arg1, arg2) {
            const ret = arg0.closest(getStringFromWasm0(arg1, arg2));
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        }, arguments); },
        __wbg_compile_6372456a133997b2: function(arg0) {
            const ret = WebAssembly.compile(arg0);
            return ret;
        },
        __wbg_composedPath_3d7ca98a55bce60f: function(arg0) {
            const ret = arg0.composedPath();
            return ret;
        },
        __wbg_construct_4e1a16de27aea5b9: function() { return handleError(function (arg0, arg1) {
            const ret = Reflect.construct(arg0, arg1);
            return ret;
        }, arguments); },
        __wbg_controller_f91423d3b117b507: function(arg0) {
            const ret = arg0.controller;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        },
        __wbg_createElement_fcbc0805de826d62: function() { return handleError(function (arg0, arg1, arg2) {
            const ret = arg0.createElement(getStringFromWasm0(arg1, arg2));
            return ret;
        }, arguments); },
        __wbg_createObjectStore_ff668af6e79f0433: function() { return handleError(function (arg0, arg1, arg2) {
            const ret = arg0.createObjectStore(getStringFromWasm0(arg1, arg2));
            return ret;
        }, arguments); },
        __wbg_createTextNode_4dad5b18435dda7c: function(arg0, arg1, arg2) {
            const ret = arg0.createTextNode(getStringFromWasm0(arg1, arg2));
            return ret;
        },
        __wbg_create_8dbb796b6159a8e4: function(arg0) {
            const ret = Object.create(arg0);
            return ret;
        },
        __wbg_ctrlKey_2e52816fa7160097: function(arg0) {
            const ret = arg0.ctrlKey;
            return ret;
        },
        __wbg_ctrlKey_50bd8324959ca786: function(arg0) {
            const ret = arg0.ctrlKey;
            return ret;
        },
        __wbg_data_328de4280640da92: function(arg0) {
            const ret = arg0.data;
            return ret;
        },
        __wbg_debug_87fd9b1a625b7efb: function(arg0) {
            console.debug(arg0);
        },
        __wbg_debug_cf53e2612c84e4ee: function(arg0, arg1, arg2, arg3) {
            console.debug(arg0, arg1, arg2, arg3);
        },
        __wbg_deleteProperty_36be13e7a282429c: function() { return handleError(function (arg0, arg1) {
            const ret = Reflect.deleteProperty(arg0, arg1);
            return ret;
        }, arguments); },
        __wbg_delete_e7e50168de5ef96e: function() { return handleError(function (arg0, arg1) {
            const ret = arg0.delete(arg1);
            return ret;
        }, arguments); },
        __wbg_detail_a90dcd774780ebf6: function(arg0) {
            const ret = arg0.detail;
            return ret;
        },
        __wbg_dispatchEvent_ca78eaf3d469bc25: function() { return handleError(function (arg0, arg1) {
            const ret = arg0.dispatchEvent(arg1);
            return ret;
        }, arguments); },
        __wbg_documentElement_b7ec99417969bfbc: function(arg0) {
            const ret = arg0.documentElement;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        },
        __wbg_document_179650d6cb13c263: function(arg0) {
            const ret = arg0.document;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
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
        __wbg_error_7ed559cd7146b49d: function(arg0, arg1) {
            console.error(arg0, arg1);
        },
        __wbg_error_becd7e1fe6ce0623: function() { return handleError(function (arg0) {
            const ret = arg0.error;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        }, arguments); },
        __wbg_execCommand_d91312cace76d267: function() { return handleError(function (arg0, arg1, arg2, arg3, arg4, arg5) {
            const ret = arg0.execCommand(getStringFromWasm0(arg1, arg2), arg3 !== 0, getStringFromWasm0(arg4, arg5));
            return ret;
        }, arguments); },
        __wbg_exports_085d8a69333b42bc: function(arg0) {
            const ret = arg0.exports;
            return ret;
        },
        __wbg_firstChild_984b883406cc95b8: function(arg0) {
            const ret = arg0.firstChild;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        },
        __wbg_focus_2f77051f98540625: function() { return handleError(function (arg0) {
            arg0.focus();
        }, arguments); },
        __wbg_getAttribute_5a601ba4718b922a: function(arg0, arg1, arg2, arg3) {
            const ret = arg1.getAttribute(getStringFromWasm0(arg2, arg3));
            var ptr1 = isLikeNone(ret) ? 0 : passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            var len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        },
        __wbg_getBoundingClientRect_e828e6c31c66dea6: function(arg0) {
            const ret = arg0.getBoundingClientRect();
            return ret;
        },
        __wbg_getComputedStyle_961681bdf7e518e8: function() { return handleError(function (arg0, arg1) {
            const ret = arg0.getComputedStyle(arg1);
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        }, arguments); },
        __wbg_getElementById_1cbd8f06dbe8eb8e: function(arg0, arg1, arg2) {
            const ret = arg0.getElementById(getStringFromWasm0(arg1, arg2));
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        },
        __wbg_getItem_b96269ddc16cf24a: function() { return handleError(function (arg0, arg1, arg2, arg3) {
            const ret = arg1.getItem(getStringFromWasm0(arg2, arg3));
            var ptr1 = isLikeNone(ret) ? 0 : passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            var len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        }, arguments); },
        __wbg_getPropertyValue_dc6b061239dad6f1: function() { return handleError(function (arg0, arg1, arg2, arg3) {
            const ret = arg1.getPropertyValue(getStringFromWasm0(arg2, arg3));
            const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        }, arguments); },
        __wbg_getRootNode_f724c3be671a9a57: function(arg0) {
            const ret = arg0.getRootNode();
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
        __wbg_hasAttribute_1fc23d9b37b6dfb1: function(arg0, arg1, arg2) {
            const ret = arg0.hasAttribute(getStringFromWasm0(arg1, arg2));
            return ret;
        },
        __wbg_hash_508149c4291ec8c2: function() { return handleError(function (arg0, arg1) {
            const ret = arg1.hash;
            const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        }, arguments); },
        __wbg_headers_cf9c80f30e2a4eff: function(arg0) {
            const ret = arg0.headers;
            return ret;
        },
        __wbg_height_96c07d9559d0200a: function(arg0) {
            const ret = arg0.height;
            return ret;
        },
        __wbg_info_79f5309d69d9c70e: function(arg0, arg1, arg2, arg3) {
            console.info(arg0, arg1, arg2, arg3);
        },
        __wbg_info_eadbe775a8e2e9eb: function(arg0) {
            console.info(arg0);
        },
        __wbg_insertBefore_9121f73148bc4f7c: function() { return handleError(function (arg0, arg1, arg2) {
            const ret = arg0.insertBefore(arg1, arg2);
            return ret;
        }, arguments); },
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
        __wbg_instanceof_CustomEvent_9b848ed45794fd3e: function(arg0) {
            let result;
            try {
                result = arg0 instanceof CustomEvent;
            } catch (_) {
                result = false;
            }
            const ret = result;
            return ret;
        },
        __wbg_instanceof_Element_beebfaab75d12d9d: function(arg0) {
            let result;
            try {
                result = arg0 instanceof Element;
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
        __wbg_instanceof_HtmlDocument_af729c5486a6c5ae: function(arg0) {
            let result;
            try {
                result = arg0 instanceof HTMLDocument;
            } catch (_) {
                result = false;
            }
            const ret = result;
            return ret;
        },
        __wbg_instanceof_HtmlElement_4493a09212d3586f: function(arg0) {
            let result;
            try {
                result = arg0 instanceof HTMLElement;
            } catch (_) {
                result = false;
            }
            const ret = result;
            return ret;
        },
        __wbg_instanceof_HtmlTextAreaElement_37795f65e16b7ed0: function(arg0) {
            let result;
            try {
                result = arg0 instanceof HTMLTextAreaElement;
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
        __wbg_instanceof_KeyboardEvent_be49f2d8e15d587a: function(arg0) {
            let result;
            try {
                result = arg0 instanceof KeyboardEvent;
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
        __wbg_instanceof_Node_d29e7ded486fd76a: function(arg0) {
            let result;
            try {
                result = arg0 instanceof Node;
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
        __wbg_instanceof_ServiceWorkerRegistration_2ad283aecb45af7e: function(arg0) {
            let result;
            try {
                result = arg0 instanceof ServiceWorkerRegistration;
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
        __wbg_instanceof_Window_05ba1ee4f6781663: function(arg0) {
            let result;
            try {
                result = arg0 instanceof Window;
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
        __wbg_isComposing_919a0fdf6ac030c9: function(arg0) {
            const ret = arg0.isComposing;
            return ret;
        },
        __wbg_isConnected_c24b5a7bb035eae0: function(arg0) {
            const ret = arg0.isConnected;
            return ret;
        },
        __wbg_is_7b9d0b289033c7de: function(arg0, arg1) {
            const ret = Object.is(arg0, arg1);
            return ret;
        },
        __wbg_item_20d060e2ba81115e: function(arg0, arg1) {
            const ret = arg0.item(arg1 >>> 0);
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        },
        __wbg_item_80aedbc6650fb95e: function(arg0, arg1) {
            const ret = arg0.item(arg1 >>> 0);
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        },
        __wbg_item_d2954a0a1297c2cf: function(arg0, arg1) {
            const ret = arg0.item(arg1 >>> 0);
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        },
        __wbg_iterator_6f722e4a93058b71: function() {
            const ret = Symbol.iterator;
            return ret;
        },
        __wbg_key_803dca86cdcfa8dd: function(arg0, arg1) {
            const ret = arg1.key;
            const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        },
        __wbg_left_7e76a74d0db1754f: function(arg0) {
            const ret = arg0.left;
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
        __wbg_length_d815d062349fe61a: function(arg0) {
            const ret = arg0.length;
            return ret;
        },
        __wbg_length_dd6ada793074ef31: function(arg0) {
            const ret = arg0.length;
            return ret;
        },
        __wbg_length_f013b2ebf7dcf709: function(arg0) {
            const ret = arg0.length;
            return ret;
        },
        __wbg_localStorage_5bf6ce3f8e51412a: function() { return handleError(function (arg0) {
            const ret = arg0.localStorage;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        }, arguments); },
        __wbg_location_c9a2271428996698: function(arg0) {
            const ret = arg0.location;
            return ret;
        },
        __wbg_mark_310e9a031ccc84ff: function() { return handleError(function (arg0, arg1, arg2) {
            arg0.mark(getStringFromWasm0(arg1, arg2));
        }, arguments); },
        __wbg_mark_742962d66a858cc5: function() { return handleError(function (arg0, arg1, arg2, arg3) {
            arg0.mark(getStringFromWasm0(arg1, arg2), arg3);
        }, arguments); },
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
        __wbg_metaKey_d961c7572a9f84f5: function(arg0) {
            const ret = arg0.metaKey;
            return ret;
        },
        __wbg_metaKey_f934f09e37889d70: function(arg0) {
            const ret = arg0.metaKey;
            return ret;
        },
        __wbg_name_d977f6ea4ad9cdcf: function(arg0, arg1) {
            const ret = arg1.name;
            const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        },
        __wbg_navigator_99621db14b3f1099: function(arg0) {
            const ret = arg0.navigator;
            return ret;
        },
        __wbg_new_0d809930cd1354c6: function() { return handleError(function () {
            const ret = new Headers();
            return ret;
        }, arguments); },
        __wbg_new_32b398fb48b6d94a: function() {
            const ret = new Array();
            return ret;
        },
        __wbg_new_5394f65338077341: function() { return handleError(function (arg0) {
            const ret = new ResizeObserver(arg0);
            return ret;
        }, arguments); },
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
        __wbg_new_c37d5116642e416a: function() { return handleError(function (arg0, arg1) {
            const ret = new Event(getStringFromWasm0(arg0, arg1));
            return ret;
        }, arguments); },
        __wbg_new_cc984128914cfc6f: function(arg0) {
            const ret = new Date(arg0);
            return ret;
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
        __wbg_new_f1a34223ddbe3f7d: function() { return handleError(function () {
            const ret = new MessageChannel();
            return ret;
        }, arguments); },
        __wbg_new_ff9311947c25f0ab: function() { return handleError(function () {
            const ret = new CSSStyleSheet();
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
        __wbg_new_with_event_init_dict_b7d5fa264299bcfa: function() { return handleError(function (arg0, arg1, arg2) {
            const ret = new CustomEvent(getStringFromWasm0(arg0, arg1), arg2);
            return ret;
        }, arguments); },
        __wbg_new_with_str_and_init_d95cbe11ce28e65e: function() { return handleError(function (arg0, arg1, arg2) {
            const ret = new Request(getStringFromWasm0(arg0, arg1), arg2);
            return ret;
        }, arguments); },
        __wbg_new_with_value_6990dd0a8bc17691: function() { return handleError(function (arg0, arg1) {
            const ret = new WebAssembly.Table(arg0, arg1);
            return ret;
        }, arguments); },
        __wbg_nextSibling_0e94ccfa3c22fa3c: function(arg0) {
            const ret = arg0.nextSibling;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        },
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
        __wbg_observe_f27242eadc2ebd47: function(arg0, arg1) {
            arg0.observe(arg1);
        },
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
        __wbg_ownerDocument_5a7a5473f8709b3e: function(arg0) {
            const ret = arg0.ownerDocument;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        },
        __wbg_parentElement_5030754e30795652: function(arg0) {
            const ret = arg0.parentElement;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        },
        __wbg_performance_757310249566272b: function() {
            const ret = globalThis.performance;
            return ret;
        },
        __wbg_port1_dabba0a56576e47e: function(arg0) {
            const ret = arg0.port1;
            return ret;
        },
        __wbg_port2_d05676aee003eedc: function(arg0) {
            const ret = arg0.port2;
            return ret;
        },
        __wbg_postMessage_1a5e42dfbec40f42: function() { return handleError(function (arg0, arg1) {
            arg0.postMessage(arg1);
        }, arguments); },
        __wbg_postMessage_95a6a0f3591209f1: function() { return handleError(function (arg0, arg1, arg2) {
            arg0.postMessage(arg1, arg2);
        }, arguments); },
        __wbg_preventDefault_b64888c857500682: function(arg0) {
            arg0.preventDefault();
        },
        __wbg_previousElementSibling_844357c0c2cc6541: function(arg0) {
            const ret = arg0.previousElementSibling;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        },
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
        __wbg_querySelectorAll_7e98cbe256deaadd: function() { return handleError(function (arg0, arg1, arg2) {
            const ret = arg0.querySelectorAll(getStringFromWasm0(arg1, arg2));
            return ret;
        }, arguments); },
        __wbg_querySelector_b966f59fa9848d69: function() { return handleError(function (arg0, arg1, arg2) {
            const ret = arg0.querySelector(getStringFromWasm0(arg1, arg2));
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        }, arguments); },
        __wbg_querySelector_fd7d157ebe17cd16: function() { return handleError(function (arg0, arg1, arg2) {
            const ret = arg0.querySelector(getStringFromWasm0(arg1, arg2));
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        }, arguments); },
        __wbg_queueMicrotask_0ab5b2d2393e99b9: function(arg0) {
            const ret = arg0.queueMicrotask;
            return ret;
        },
        __wbg_queueMicrotask_6a09b7bc46549209: function(arg0) {
            queueMicrotask(arg0);
        },
        __wbg_ready_478ecee34f25ae01: function() { return handleError(function (arg0) {
            const ret = arg0.ready;
            return ret;
        }, arguments); },
        __wbg_register_b5c5103be75f5ced: function(arg0, arg1, arg2) {
            const ret = arg0.register(getStringFromWasm0(arg1, arg2));
            return ret;
        },
        __wbg_reject_90df95b492dd8563: function(arg0) {
            const ret = Promise.reject(arg0);
            return ret;
        },
        __wbg_removeAttribute_1e7d2c409776d836: function() { return handleError(function (arg0, arg1, arg2) {
            arg0.removeAttribute(getStringFromWasm0(arg1, arg2));
        }, arguments); },
        __wbg_removeChild_8d9536328d674d54: function() { return handleError(function (arg0, arg1) {
            const ret = arg0.removeChild(arg1);
            return ret;
        }, arguments); },
        __wbg_removeEventListener_a3f23c70077bdcc1: function() { return handleError(function (arg0, arg1, arg2, arg3) {
            arg0.removeEventListener(getStringFromWasm0(arg1, arg2), arg3);
        }, arguments); },
        __wbg_removeEventListener_c0e097844dc1021c: function() { return handleError(function (arg0, arg1, arg2, arg3, arg4) {
            arg0.removeEventListener(getStringFromWasm0(arg1, arg2), arg3, arg4 !== 0);
        }, arguments); },
        __wbg_remove_ce1b54059317fe8a: function(arg0) {
            arg0.remove();
        },
        __wbg_replaceChildren_88e2eec72b85f12e: function(arg0, arg1) {
            arg0.replaceChildren(...arg1);
        },
        __wbg_replaceSync_4449064c4c827515: function() { return handleError(function (arg0, arg1, arg2) {
            arg0.replaceSync(getStringFromWasm0(arg1, arg2));
        }, arguments); },
        __wbg_resolve_2191a4dfe481c25b: function(arg0) {
            const ret = Promise.resolve(arg0);
            return ret;
        },
        __wbg_result_2b1294a2bf8dc773: function() { return handleError(function (arg0) {
            const ret = arg0.result;
            return ret;
        }, arguments); },
        __wbg_scrollIntoView_8aebc47f4e6dd724: function(arg0, arg1) {
            arg0.scrollIntoView(arg1 !== 0);
        },
        __wbg_scrollLeft_5be50f489b342a09: function(arg0) {
            const ret = arg0.scrollLeft;
            return ret;
        },
        __wbg_scrollTop_dc5ff575e4a53628: function(arg0) {
            const ret = arg0.scrollTop;
            return ret;
        },
        __wbg_selectionEnd_2b070cc8ee6df59d: function() { return handleError(function (arg0) {
            const ret = arg0.selectionEnd;
            return isLikeNone(ret) ? Number.MAX_SAFE_INTEGER : (ret) >>> 0;
        }, arguments); },
        __wbg_selectionStart_90c5f80adb84005d: function() { return handleError(function (arg0) {
            const ret = arg0.selectionStart;
            return isLikeNone(ret) ? Number.MAX_SAFE_INTEGER : (ret) >>> 0;
        }, arguments); },
        __wbg_serviceWorker_d4bc7943897ac9b4: function(arg0) {
            const ret = arg0.serviceWorker;
            return ret;
        },
        __wbg_setAttribute_71039043be82d098: function() { return handleError(function (arg0, arg1, arg2, arg3, arg4) {
            arg0.setAttribute(getStringFromWasm0(arg1, arg2), getStringFromWasm0(arg3, arg4));
        }, arguments); },
        __wbg_setInterval_93ec7461c3650c76: function() { return handleError(function (arg0, arg1, arg2) {
            const ret = arg0.setInterval(arg1, arg2);
            return ret;
        }, arguments); },
        __wbg_setItem_364a11cf21db9039: function() { return handleError(function (arg0, arg1, arg2, arg3, arg4) {
            arg0.setItem(getStringFromWasm0(arg1, arg2), getStringFromWasm0(arg3, arg4));
        }, arguments); },
        __wbg_setProperty_e4e51b1b1d681d15: function() { return handleError(function (arg0, arg1, arg2, arg3, arg4) {
            arg0.setProperty(getStringFromWasm0(arg1, arg2), getStringFromWasm0(arg3, arg4));
        }, arguments); },
        __wbg_setRangeText_f1209bb8c9a58ac2: function() { return handleError(function (arg0, arg1, arg2, arg3, arg4) {
            arg0.setRangeText(getStringFromWasm0(arg1, arg2), arg3 >>> 0, arg4 >>> 0);
        }, arguments); },
        __wbg_setSelectionRange_818aa138eca239df: function() { return handleError(function (arg0, arg1, arg2) {
            arg0.setSelectionRange(arg1 >>> 0, arg2 >>> 0);
        }, arguments); },
        __wbg_setTimeout_cfa2cf195c3738db: function() { return handleError(function (arg0, arg1, arg2) {
            const ret = arg0.setTimeout(arg1, arg2);
            return ret;
        }, arguments); },
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
        __wbg_set_body_029f2d171e0a005f: function(arg0, arg1) {
            arg0.body = arg1;
        },
        __wbg_set_bubbles_b4e5c994b4f96f0e: function(arg0, arg1) {
            arg0.bubbles = arg1 !== 0;
        },
        __wbg_set_capture_cd03f1518ee1dafb: function(arg0, arg1) {
            arg0.capture = arg1 !== 0;
        },
        __wbg_set_composed_4b360b2fea1c1905: function(arg0, arg1) {
            arg0.composed = arg1 !== 0;
        },
        __wbg_set_detail_564ad103413aa5dc: function(arg0, arg1) {
            arg0.detail = arg1;
        },
        __wbg_set_headers_9c61d123c3ee1f10: function(arg0, arg1) {
            arg0.headers = arg1;
        },
        __wbg_set_innerHTML_f78a45a07f97e136: function(arg0, arg1, arg2) {
            arg0.innerHTML = getStringFromWasm0(arg1, arg2);
        },
        __wbg_set_method_5532d59b92d76467: function(arg0, arg1, arg2) {
            arg0.method = getStringFromWasm0(arg1, arg2);
        },
        __wbg_set_once_51a9fb6b8af8a72b: function(arg0, arg1) {
            arg0.once = arg1 !== 0;
        },
        __wbg_set_onmessage_d511b70365304094: function(arg0, arg1) {
            arg0.onmessage = arg1;
        },
        __wbg_set_onupgradeneeded_7b2cf4ba1c57e655: function(arg0, arg1) {
            arg0.onupgradeneeded = arg1;
        },
        __wbg_set_scrollLeft_075e81a16fc93cca: function(arg0, arg1) {
            arg0.scrollLeft = arg1;
        },
        __wbg_set_scrollTop_431b03eeb3e9c636: function(arg0, arg1) {
            arg0.scrollTop = arg1;
        },
        __wbg_set_textContent_54dcad83ae15772d: function(arg0, arg1, arg2) {
            arg0.textContent = arg1 === 0 ? undefined : getStringFromWasm0(arg1, arg2);
        },
        __wbg_set_value_a342f135c825187d: function(arg0, arg1, arg2) {
            arg0.value = getStringFromWasm0(arg1, arg2);
        },
        __wbg_set_value_f86d819107cb271b: function(arg0, arg1) {
            arg0.value = arg1;
        },
        __wbg_shadowRoot_9a0f635b93f2c2a0: function(arg0) {
            const ret = arg0.shadowRoot;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        },
        __wbg_shiftKey_9bcb8bdd60c2f152: function(arg0) {
            const ret = arg0.shiftKey;
            return ret;
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
        __wbg_stopPropagation_4c4ff88c29f9bc38: function(arg0) {
            arg0.stopPropagation();
        },
        __wbg_style_6657aed849e5d757: function(arg0) {
            const ret = arg0.style;
            return ret;
        },
        __wbg_tagName_d99c8072027f3c98: function(arg0, arg1) {
            const ret = arg1.tagName;
            const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        },
        __wbg_target_40228f69bb78cd78: function(arg0) {
            const ret = arg0.target;
            return ret;
        },
        __wbg_target_e759594a8d965ed7: function(arg0) {
            const ret = arg0.target;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        },
        __wbg_textContent_37277f66248f39e6: function(arg0, arg1) {
            const ret = arg1.textContent;
            var ptr1 = isLikeNone(ret) ? 0 : passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            var len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        },
        __wbg_then_16d107c451e9905d: function(arg0, arg1, arg2) {
            const ret = arg0.then(arg1, arg2);
            return ret;
        },
        __wbg_then_6ec10ae38b3e92f7: function(arg0, arg1) {
            const ret = arg0.then(arg1);
            return ret;
        },
        __wbg_toLocaleTimeString_7ce08015b8452f2e: function(arg0, arg1, arg2) {
            const ret = arg0.toLocaleTimeString(getStringFromWasm0(arg1, arg2));
            return ret;
        },
        __wbg_top_fe120acfa924a430: function(arg0) {
            const ret = arg0.top;
            return ret;
        },
        __wbg_transaction_d911d96b4b0af154: function() { return handleError(function (arg0, arg1, arg2, arg3) {
            const ret = arg0.transaction(getStringFromWasm0(arg1, arg2), __wbindgen_enum_IdbTransactionMode[arg3]);
            return ret;
        }, arguments); },
        __wbg_type_9d13c17ea2611dd0: function(arg0, arg1) {
            const ret = arg1.type;
            const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        },
        __wbg_unobserve_41a4f85162f1fb18: function(arg0, arg1) {
            arg0.unobserve(arg1);
        },
        __wbg_validate_2bf76e0e61374b8e: function() { return handleError(function (arg0) {
            const ret = WebAssembly.validate(arg0);
            return ret;
        }, arguments); },
        __wbg_value_5bf3641a3c74fb47: function(arg0) {
            const ret = arg0.value;
            return ret;
        },
        __wbg_value_5fb5e3598c0d37c5: function(arg0, arg1) {
            const ret = arg1.value;
            const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        },
        __wbg_value_a5d5488a9589444a: function(arg0) {
            const ret = arg0.value;
            return ret;
        },
        __wbg_value_c40f8f7227bb3a9e: function(arg0, arg1) {
            const ret = arg1.value;
            const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        },
        __wbg_warn_77c4eb4a21e10a21: function(arg0, arg1, arg2, arg3) {
            console.warn(arg0, arg1, arg2, arg3);
        },
        __wbg_warn_b1370d804fa3e259: function(arg0) {
            console.warn(arg0);
        },
        __wbg_width_219185400361db86: function(arg0) {
            const ret = arg0.width;
            return ret;
        },
        __wbindgen_cast_0000000000000001: function(arg0, arg1) {
            // Cast intrinsic for `Closure(Closure { owned: true, function: Function { arguments: [Externref], shim_idx: 1613, ret: Result(Unit), inner_ret: Some(Result(Unit)) }, mutable: false }) -> Externref`.
            const ret = makeClosure(arg0, arg1, wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___wasm_bindgen_7147a55964b576c1___JsValue__core_37ee0ce0c4cd57d5___result__Result_____wasm_bindgen_7147a55964b576c1___JsValue___true_);
            return ret;
        },
        __wbindgen_cast_0000000000000002: function(arg0, arg1) {
            // Cast intrinsic for `Closure(Closure { owned: true, function: Function { arguments: [Externref], shim_idx: 1631, ret: Unit, inner_ret: Some(Unit) }, mutable: true }) -> Externref`.
            const ret = makeMutClosure(arg0, arg1, wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___wasm_bindgen_7147a55964b576c1___JsValue______true_);
            return ret;
        },
        __wbindgen_cast_0000000000000003: function(arg0, arg1) {
            // Cast intrinsic for `Closure(Closure { owned: true, function: Function { arguments: [Externref], shim_idx: 2297, ret: Result(Unit), inner_ret: Some(Result(Unit)) }, mutable: true }) -> Externref`.
            const ret = makeMutClosure(arg0, arg1, wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___wasm_bindgen_7147a55964b576c1___JsValue__core_37ee0ce0c4cd57d5___result__Result_____wasm_bindgen_7147a55964b576c1___JsError___true_);
            return ret;
        },
        __wbindgen_cast_0000000000000004: function(arg0, arg1) {
            // Cast intrinsic for `Closure(Closure { owned: true, function: Function { arguments: [NamedExternref("Array<any>")], shim_idx: 1535, ret: Unit, inner_ret: Some(Unit) }, mutable: false }) -> Externref`.
            const ret = makeClosure(arg0, arg1, wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___js_sys_6b1da905d9f5533e___Array______true_);
            return ret;
        },
        __wbindgen_cast_0000000000000005: function(arg0, arg1) {
            // Cast intrinsic for `Closure(Closure { owned: true, function: Function { arguments: [NamedExternref("Event")], shim_idx: 1535, ret: Unit, inner_ret: Some(Unit) }, mutable: false }) -> Externref`.
            const ret = makeClosure(arg0, arg1, wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___js_sys_6b1da905d9f5533e___Array______true__4);
            return ret;
        },
        __wbindgen_cast_0000000000000006: function(arg0, arg1) {
            // Cast intrinsic for `Closure(Closure { owned: true, function: Function { arguments: [NamedExternref("Event")], shim_idx: 1554, ret: Unit, inner_ret: Some(Unit) }, mutable: true }) -> Externref`.
            const ret = makeMutClosure(arg0, arg1, wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___web_sys_e24fb17a5cad9f88___features__gen_MessageEvent__MessageEvent______true_);
            return ret;
        },
        __wbindgen_cast_0000000000000007: function(arg0, arg1) {
            // Cast intrinsic for `Closure(Closure { owned: true, function: Function { arguments: [NamedExternref("HTMLElement"), String, Option(String)], shim_idx: 1544, ret: Unit, inner_ret: Some(Unit) }, mutable: false }) -> Externref`.
            const ret = makeClosure(arg0, arg1, wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___web_sys_e24fb17a5cad9f88___features__gen_HtmlElement__HtmlElement__alloc_42d2b33bd937efdb___string__String__core_37ee0ce0c4cd57d5___option__Option_alloc_42d2b33bd937efdb___string__String_______true_);
            return ret;
        },
        __wbindgen_cast_0000000000000008: function(arg0, arg1) {
            // Cast intrinsic for `Closure(Closure { owned: true, function: Function { arguments: [NamedExternref("HTMLElement")], shim_idx: 1535, ret: Unit, inner_ret: Some(Unit) }, mutable: false }) -> Externref`.
            const ret = makeClosure(arg0, arg1, wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___js_sys_6b1da905d9f5533e___Array______true__7);
            return ret;
        },
        __wbindgen_cast_0000000000000009: function(arg0, arg1) {
            // Cast intrinsic for `Closure(Closure { owned: true, function: Function { arguments: [NamedExternref("InputEvent")], shim_idx: 1535, ret: Unit, inner_ret: Some(Unit) }, mutable: false }) -> Externref`.
            const ret = makeClosure(arg0, arg1, wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___js_sys_6b1da905d9f5533e___Array______true__8);
            return ret;
        },
        __wbindgen_cast_000000000000000a: function(arg0, arg1) {
            // Cast intrinsic for `Closure(Closure { owned: true, function: Function { arguments: [NamedExternref("KeyboardEvent")], shim_idx: 1535, ret: Unit, inner_ret: Some(Unit) }, mutable: false }) -> Externref`.
            const ret = makeClosure(arg0, arg1, wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___js_sys_6b1da905d9f5533e___Array______true__9);
            return ret;
        },
        __wbindgen_cast_000000000000000b: function(arg0, arg1) {
            // Cast intrinsic for `Closure(Closure { owned: true, function: Function { arguments: [NamedExternref("MessageEvent")], shim_idx: 1535, ret: Unit, inner_ret: Some(Unit) }, mutable: false }) -> Externref`.
            const ret = makeClosure(arg0, arg1, wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___js_sys_6b1da905d9f5533e___Array______true__10);
            return ret;
        },
        __wbindgen_cast_000000000000000c: function(arg0, arg1) {
            // Cast intrinsic for `Closure(Closure { owned: true, function: Function { arguments: [NamedExternref("MessageEvent")], shim_idx: 1554, ret: Unit, inner_ret: Some(Unit) }, mutable: true }) -> Externref`.
            const ret = makeMutClosure(arg0, arg1, wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___web_sys_e24fb17a5cad9f88___features__gen_MessageEvent__MessageEvent______true__11);
            return ret;
        },
        __wbindgen_cast_000000000000000d: function(arg0, arg1) {
            // Cast intrinsic for `Closure(Closure { owned: true, function: Function { arguments: [NamedExternref("MouseEvent")], shim_idx: 1535, ret: Unit, inner_ret: Some(Unit) }, mutable: false }) -> Externref`.
            const ret = makeClosure(arg0, arg1, wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___js_sys_6b1da905d9f5533e___Array______true__12);
            return ret;
        },
        __wbindgen_cast_000000000000000e: function(arg0, arg1) {
            // Cast intrinsic for `Closure(Closure { owned: true, function: Function { arguments: [String, String], shim_idx: 1537, ret: NamedExternref("Promise<any>"), inner_ret: Some(NamedExternref("Promise<any>")) }, mutable: false }) -> Externref`.
            const ret = makeClosure(arg0, arg1, wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___alloc_42d2b33bd937efdb___string__String__alloc_42d2b33bd937efdb___string__String__js_sys_6b1da905d9f5533e___Promise__true_);
            return ret;
        },
        __wbindgen_cast_000000000000000f: function(arg0, arg1) {
            // Cast intrinsic for `Closure(Closure { owned: true, function: Function { arguments: [String], shim_idx: 1539, ret: NamedExternref("Promise<any>"), inner_ret: Some(NamedExternref("Promise<any>")) }, mutable: false }) -> Externref`.
            const ret = makeClosure(arg0, arg1, wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___alloc_42d2b33bd937efdb___string__String__js_sys_6b1da905d9f5533e___Promise__true_);
            return ret;
        },
        __wbindgen_cast_0000000000000010: function(arg0, arg1) {
            // Cast intrinsic for `Closure(Closure { owned: true, function: Function { arguments: [U32, Externref, Externref, Externref, Externref], shim_idx: 1848, ret: I32, inner_ret: Some(I32) }, mutable: false }) -> Externref`.
            const ret = makeClosure(arg0, arg1, wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___u32__wasm_bindgen_7147a55964b576c1___JsValue__wasm_bindgen_7147a55964b576c1___JsValue__wasm_bindgen_7147a55964b576c1___JsValue__wasm_bindgen_7147a55964b576c1___JsValue__i32__true_);
            return ret;
        },
        __wbindgen_cast_0000000000000011: function(arg0, arg1) {
            // Cast intrinsic for `Closure(Closure { owned: true, function: Function { arguments: [U32, Externref, Externref, Externref], shim_idx: 1622, ret: I32, inner_ret: Some(I32) }, mutable: false }) -> Externref`.
            const ret = makeClosure(arg0, arg1, wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___u32__wasm_bindgen_7147a55964b576c1___JsValue__wasm_bindgen_7147a55964b576c1___JsValue__wasm_bindgen_7147a55964b576c1___JsValue__i32__true_);
            return ret;
        },
        __wbindgen_cast_0000000000000012: function(arg0, arg1) {
            // Cast intrinsic for `Closure(Closure { owned: true, function: Function { arguments: [U32, Externref, Externref], shim_idx: 1624, ret: I32, inner_ret: Some(I32) }, mutable: false }) -> Externref`.
            const ret = makeClosure(arg0, arg1, wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___u32__wasm_bindgen_7147a55964b576c1___JsValue__wasm_bindgen_7147a55964b576c1___JsValue__i32__true_);
            return ret;
        },
        __wbindgen_cast_0000000000000013: function(arg0, arg1) {
            // Cast intrinsic for `Closure(Closure { owned: true, function: Function { arguments: [U32, Externref], shim_idx: 1626, ret: I32, inner_ret: Some(I32) }, mutable: false }) -> Externref`.
            const ret = makeClosure(arg0, arg1, wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___u32__wasm_bindgen_7147a55964b576c1___JsValue__i32__true_);
            return ret;
        },
        __wbindgen_cast_0000000000000014: function(arg0, arg1) {
            // Cast intrinsic for `Closure(Closure { owned: true, function: Function { arguments: [U32, Externref], shim_idx: 1628, ret: Unit, inner_ret: Some(Unit) }, mutable: false }) -> Externref`.
            const ret = makeClosure(arg0, arg1, wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___u32__wasm_bindgen_7147a55964b576c1___JsValue______true_);
            return ret;
        },
        __wbindgen_cast_0000000000000015: function(arg0, arg1) {
            // Cast intrinsic for `Closure(Closure { owned: true, function: Function { arguments: [U32, U32], shim_idx: 1626, ret: Externref, inner_ret: Some(Externref) }, mutable: false }) -> Externref`.
            const ret = makeClosure(arg0, arg1, wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___u32__wasm_bindgen_7147a55964b576c1___JsValue__i32__true__20);
            return ret;
        },
        __wbindgen_cast_0000000000000016: function(arg0, arg1) {
            // Cast intrinsic for `Closure(Closure { owned: true, function: Function { arguments: [U32], shim_idx: 1615, ret: Externref, inner_ret: Some(Externref) }, mutable: false }) -> Externref`.
            const ret = makeClosure(arg0, arg1, wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___u32__wasm_bindgen_7147a55964b576c1___JsValue__true_);
            return ret;
        },
        __wbindgen_cast_0000000000000017: function(arg0, arg1) {
            // Cast intrinsic for `Closure(Closure { owned: true, function: Function { arguments: [U32], shim_idx: 1615, ret: I32, inner_ret: Some(I32) }, mutable: false }) -> Externref`.
            const ret = makeClosure(arg0, arg1, wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___u32__wasm_bindgen_7147a55964b576c1___JsValue__true__22);
            return ret;
        },
        __wbindgen_cast_0000000000000018: function(arg0, arg1) {
            // Cast intrinsic for `Closure(Closure { owned: true, function: Function { arguments: [U32], shim_idx: 1615, ret: U32, inner_ret: Some(U32) }, mutable: false }) -> Externref`.
            const ret = makeClosure(arg0, arg1, wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___u32__wasm_bindgen_7147a55964b576c1___JsValue__true__23);
            return ret;
        },
        __wbindgen_cast_0000000000000019: function(arg0, arg1) {
            // Cast intrinsic for `Closure(Closure { owned: true, function: Function { arguments: [U32], shim_idx: 1619, ret: Unit, inner_ret: Some(Unit) }, mutable: false }) -> Externref`.
            const ret = makeClosure(arg0, arg1, wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___u32______true_);
            return ret;
        },
        __wbindgen_cast_000000000000001a: function(arg0, arg1) {
            // Cast intrinsic for `Closure(Closure { owned: true, function: Function { arguments: [], shim_idx: 1549, ret: Externref, inner_ret: Some(Externref) }, mutable: false }) -> Externref`.
            const ret = makeClosure(arg0, arg1, wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___wasm_bindgen_7147a55964b576c1___JsValue__true_);
            return ret;
        },
        __wbindgen_cast_000000000000001b: function(arg0, arg1) {
            // Cast intrinsic for `Closure(Closure { owned: true, function: Function { arguments: [], shim_idx: 1549, ret: NamedExternref("Promise<any>"), inner_ret: Some(NamedExternref("Promise<any>")) }, mutable: false }) -> Externref`.
            const ret = makeClosure(arg0, arg1, wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___wasm_bindgen_7147a55964b576c1___JsValue__true__26);
            return ret;
        },
        __wbindgen_cast_000000000000001c: function(arg0, arg1) {
            // Cast intrinsic for `Closure(Closure { owned: true, function: Function { arguments: [], shim_idx: 1552, ret: Unit, inner_ret: Some(Unit) }, mutable: false }) -> Externref`.
            const ret = makeClosure(arg0, arg1, wasm_bindgen_7147a55964b576c1___convert__closures_____invoke_______true_);
            return ret;
        },
        __wbindgen_cast_000000000000001d: function(arg0, arg1) {
            // Cast intrinsic for `Closure(Closure { owned: true, function: Function { arguments: [], shim_idx: 1633, ret: Unit, inner_ret: Some(Unit) }, mutable: true }) -> Externref`.
            const ret = makeMutClosure(arg0, arg1, wasm_bindgen_7147a55964b576c1___convert__closures_____invoke_______true__1_);
            return ret;
        },
        __wbindgen_cast_000000000000001e: function(arg0, arg1) {
            // Cast intrinsic for `Closure(Closure { owned: true, function: Function { arguments: [], shim_idx: 2201, ret: Unit, inner_ret: Some(Unit) }, mutable: true }) -> Externref`.
            const ret = makeMutClosure(arg0, arg1, wasm_bindgen_7147a55964b576c1___convert__closures_____invoke_______true__2_);
            return ret;
        },
        __wbindgen_cast_000000000000001f: function(arg0) {
            // Cast intrinsic for `F64 -> Externref`.
            const ret = arg0;
            return ret;
        },
        __wbindgen_cast_0000000000000020: function(arg0) {
            // Cast intrinsic for `I64 -> Externref`.
            const ret = arg0;
            return ret;
        },
        __wbindgen_cast_0000000000000021: function(arg0, arg1) {
            // Cast intrinsic for `Ref(String) -> Externref`.
            const ret = getStringFromWasm0(arg0, arg1);
            return ret;
        },
        __wbindgen_cast_0000000000000022: function(arg0) {
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
        "./conduct-demo-page_bg.js": import0,
    };
}

function wasm_bindgen_7147a55964b576c1___convert__closures_____invoke_______true_(arg0, arg1) {
    wasm.wasm_bindgen_7147a55964b576c1___convert__closures_____invoke_______true_(arg0, arg1);
}

function wasm_bindgen_7147a55964b576c1___convert__closures_____invoke_______true__1_(arg0, arg1) {
    wasm.wasm_bindgen_7147a55964b576c1___convert__closures_____invoke_______true__1_(arg0, arg1);
}

function wasm_bindgen_7147a55964b576c1___convert__closures_____invoke_______true__2_(arg0, arg1) {
    wasm.wasm_bindgen_7147a55964b576c1___convert__closures_____invoke_______true__2_(arg0, arg1);
}

function wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___wasm_bindgen_7147a55964b576c1___JsValue__true_(arg0, arg1) {
    const ret = wasm.wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___wasm_bindgen_7147a55964b576c1___JsValue__true_(arg0, arg1);
    return ret;
}

function wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___wasm_bindgen_7147a55964b576c1___JsValue__true__26(arg0, arg1) {
    const ret = wasm.wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___wasm_bindgen_7147a55964b576c1___JsValue__true__26(arg0, arg1);
    return ret;
}

function wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___wasm_bindgen_7147a55964b576c1___JsValue______true_(arg0, arg1, arg2) {
    wasm.wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___wasm_bindgen_7147a55964b576c1___JsValue______true_(arg0, arg1, arg2);
}

function wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___js_sys_6b1da905d9f5533e___Array______true_(arg0, arg1, arg2) {
    wasm.wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___js_sys_6b1da905d9f5533e___Array______true_(arg0, arg1, arg2);
}

function wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___js_sys_6b1da905d9f5533e___Array______true__4(arg0, arg1, arg2) {
    wasm.wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___js_sys_6b1da905d9f5533e___Array______true__4(arg0, arg1, arg2);
}

function wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___web_sys_e24fb17a5cad9f88___features__gen_MessageEvent__MessageEvent______true_(arg0, arg1, arg2) {
    wasm.wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___web_sys_e24fb17a5cad9f88___features__gen_MessageEvent__MessageEvent______true_(arg0, arg1, arg2);
}

function wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___js_sys_6b1da905d9f5533e___Array______true__7(arg0, arg1, arg2) {
    wasm.wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___js_sys_6b1da905d9f5533e___Array______true__7(arg0, arg1, arg2);
}

function wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___js_sys_6b1da905d9f5533e___Array______true__8(arg0, arg1, arg2) {
    wasm.wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___js_sys_6b1da905d9f5533e___Array______true__8(arg0, arg1, arg2);
}

function wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___js_sys_6b1da905d9f5533e___Array______true__9(arg0, arg1, arg2) {
    wasm.wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___js_sys_6b1da905d9f5533e___Array______true__9(arg0, arg1, arg2);
}

function wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___js_sys_6b1da905d9f5533e___Array______true__10(arg0, arg1, arg2) {
    wasm.wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___js_sys_6b1da905d9f5533e___Array______true__10(arg0, arg1, arg2);
}

function wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___web_sys_e24fb17a5cad9f88___features__gen_MessageEvent__MessageEvent______true__11(arg0, arg1, arg2) {
    wasm.wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___web_sys_e24fb17a5cad9f88___features__gen_MessageEvent__MessageEvent______true__11(arg0, arg1, arg2);
}

function wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___js_sys_6b1da905d9f5533e___Array______true__12(arg0, arg1, arg2) {
    wasm.wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___js_sys_6b1da905d9f5533e___Array______true__12(arg0, arg1, arg2);
}

function wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___wasm_bindgen_7147a55964b576c1___JsValue__core_37ee0ce0c4cd57d5___result__Result_____wasm_bindgen_7147a55964b576c1___JsValue___true_(arg0, arg1, arg2) {
    const ret = wasm.wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___wasm_bindgen_7147a55964b576c1___JsValue__core_37ee0ce0c4cd57d5___result__Result_____wasm_bindgen_7147a55964b576c1___JsValue___true_(arg0, arg1, arg2);
    if (ret[1]) {
        throw takeFromExternrefTable0(ret[0]);
    }
}

function wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___wasm_bindgen_7147a55964b576c1___JsValue__core_37ee0ce0c4cd57d5___result__Result_____wasm_bindgen_7147a55964b576c1___JsError___true_(arg0, arg1, arg2) {
    const ret = wasm.wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___wasm_bindgen_7147a55964b576c1___JsValue__core_37ee0ce0c4cd57d5___result__Result_____wasm_bindgen_7147a55964b576c1___JsError___true_(arg0, arg1, arg2);
    if (ret[1]) {
        throw takeFromExternrefTable0(ret[0]);
    }
}

function wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___js_sys_6b1da905d9f5533e___Function_fn_wasm_bindgen_7147a55964b576c1___JsValue_____wasm_bindgen_7147a55964b576c1___sys__Undefined___js_sys_6b1da905d9f5533e___Function_fn_wasm_bindgen_7147a55964b576c1___JsValue_____wasm_bindgen_7147a55964b576c1___sys__Undefined_______true_(arg0, arg1, arg2, arg3) {
    wasm.wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___js_sys_6b1da905d9f5533e___Function_fn_wasm_bindgen_7147a55964b576c1___JsValue_____wasm_bindgen_7147a55964b576c1___sys__Undefined___js_sys_6b1da905d9f5533e___Function_fn_wasm_bindgen_7147a55964b576c1___JsValue_____wasm_bindgen_7147a55964b576c1___sys__Undefined_______true_(arg0, arg1, arg2, arg3);
}

function wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___web_sys_e24fb17a5cad9f88___features__gen_HtmlElement__HtmlElement__alloc_42d2b33bd937efdb___string__String__core_37ee0ce0c4cd57d5___option__Option_alloc_42d2b33bd937efdb___string__String_______true_(arg0, arg1, arg2, arg3, arg4) {
    const ptr0 = passStringToWasm0(arg3, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len0 = WASM_VECTOR_LEN;
    var ptr1 = isLikeNone(arg4) ? 0 : passStringToWasm0(arg4, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    var len1 = WASM_VECTOR_LEN;
    wasm.wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___web_sys_e24fb17a5cad9f88___features__gen_HtmlElement__HtmlElement__alloc_42d2b33bd937efdb___string__String__core_37ee0ce0c4cd57d5___option__Option_alloc_42d2b33bd937efdb___string__String_______true_(arg0, arg1, arg2, ptr0, len0, ptr1, len1);
}

function wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___u32______true_(arg0, arg1, arg2) {
    wasm.wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___u32______true_(arg0, arg1, arg2);
}

function wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___u32__wasm_bindgen_7147a55964b576c1___JsValue__true_(arg0, arg1, arg2) {
    const ret = wasm.wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___u32__wasm_bindgen_7147a55964b576c1___JsValue__true_(arg0, arg1, arg2);
    return ret;
}

function wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___u32__wasm_bindgen_7147a55964b576c1___JsValue__true__22(arg0, arg1, arg2) {
    const ret = wasm.wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___u32__wasm_bindgen_7147a55964b576c1___JsValue__true__22(arg0, arg1, arg2);
    return ret;
}

function wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___u32__wasm_bindgen_7147a55964b576c1___JsValue__true__23(arg0, arg1, arg2) {
    const ret = wasm.wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___u32__wasm_bindgen_7147a55964b576c1___JsValue__true__23(arg0, arg1, arg2);
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

function wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___alloc_42d2b33bd937efdb___string__String__js_sys_6b1da905d9f5533e___Promise__true_(arg0, arg1, arg2) {
    const ptr0 = passStringToWasm0(arg2, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len0 = WASM_VECTOR_LEN;
    const ret = wasm.wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___alloc_42d2b33bd937efdb___string__String__js_sys_6b1da905d9f5533e___Promise__true_(arg0, arg1, ptr0, len0);
    return ret;
}

function wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___u32__wasm_bindgen_7147a55964b576c1___JsValue__i32__true__20(arg0, arg1, arg2, arg3) {
    const ret = wasm.wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___u32__wasm_bindgen_7147a55964b576c1___JsValue__i32__true__20(arg0, arg1, arg2, arg3);
    return ret;
}

function wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___alloc_42d2b33bd937efdb___string__String__alloc_42d2b33bd937efdb___string__String__js_sys_6b1da905d9f5533e___Promise__true_(arg0, arg1, arg2, arg3) {
    const ptr0 = passStringToWasm0(arg2, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len0 = WASM_VECTOR_LEN;
    const ptr1 = passStringToWasm0(arg3, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len1 = WASM_VECTOR_LEN;
    const ret = wasm.wasm_bindgen_7147a55964b576c1___convert__closures_____invoke___alloc_42d2b33bd937efdb___string__String__alloc_42d2b33bd937efdb___string__String__js_sys_6b1da905d9f5533e___Promise__true_(arg0, arg1, ptr0, len0, ptr1, len1);
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
const MAX_SAFARI_DECODE_BYTES = 2146435072;
let numBytesDecoded = 0;
function decodeText(ptr, len) {
    numBytesDecoded += len;
    if (numBytesDecoded >= MAX_SAFARI_DECODE_BYTES) {
        cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true, fatal: true });
        cachedTextDecoder.decode();
        numBytesDecoded = len;
    }
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

    if (module_or_path === undefined) {
        module_or_path = new URL('conduct-demo-page_bg.wasm', import.meta.url);
    }
    const imports = __wbg_get_imports();

    if (typeof module_or_path === 'string' || (typeof Request === 'function' && module_or_path instanceof Request) || (typeof URL === 'function' && module_or_path instanceof URL)) {
        module_or_path = fetch(module_or_path);
    }

    const { instance, module } = await __wbg_load(await module_or_path, imports);

    return __wbg_finalize_init(instance, module);
}

export { initSync, __wbg_init as default };
