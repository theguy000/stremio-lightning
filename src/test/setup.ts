function createStorage(): Storage {
  const values = new Map<string, string>();

  return {
    get length() {
      return values.size;
    },
    clear() {
      values.clear();
    },
    getItem(key: string) {
      return values.get(String(key)) ?? null;
    },
    key(index: number) {
      return Array.from(values.keys())[index] ?? null;
    },
    removeItem(key: string) {
      values.delete(String(key));
    },
    setItem(key: string, value: string) {
      values.set(String(key), String(value));
    },
  };
}

function isUsableStorage(storage: unknown): storage is Storage {
  return (
    typeof storage === 'object' &&
    storage !== null &&
    typeof (storage as Storage).getItem === 'function' &&
    typeof (storage as Storage).setItem === 'function' &&
    typeof (storage as Storage).removeItem === 'function' &&
    typeof (storage as Storage).clear === 'function'
  );
}

function getValueStorage(target: object): Storage | undefined {
  const descriptor = Object.getOwnPropertyDescriptor(target, 'localStorage');
  return descriptor && 'value' in descriptor && isUsableStorage(descriptor.value)
    ? descriptor.value
    : undefined;
}

const storage = getValueStorage(globalThis) ?? createStorage();

if (getValueStorage(globalThis) !== storage) {
  Object.defineProperty(globalThis, 'localStorage', {
    configurable: true,
    value: storage,
    writable: true,
  });
}

if (typeof window !== 'undefined' && !getValueStorage(window)) {
  Object.defineProperty(window, 'localStorage', {
    configurable: true,
    value: storage,
    writable: true,
  });
}
