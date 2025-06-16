import '@testing-library/jest-dom';
import { beforeAll, vi } from 'vitest';
import { h } from 'preact';

// Mock react-bootstrap components with simple HTML elements
vi.mock('react-bootstrap', () => {
  const Modal = ({ children, show, ...props }: any) =>
    show ? h('div', { ...props, role: 'dialog' }, children) : null;

  Modal.Header = ({ children, ...props }: any) =>
    h('div', { ...props, className: 'modal-header' }, children);
  Modal.Body = ({ children, ...props }: any) =>
    h('div', { ...props, className: 'modal-body' }, children);
  Modal.Footer = ({ children, ...props }: any) =>
    h('div', { ...props, className: 'modal-footer' }, children);

  const Card = ({ children, ...props }: any) =>
    h('div', { ...props, className: 'card' }, children);

  Card.Header = ({ children, ...props }: any) =>
    h('div', { ...props, className: 'card-header' }, children);
  Card.Body = ({ children, ...props }: any) =>
    h('div', { ...props, className: 'card-body' }, children);

  const Form = ({ children, ...props }: any) =>
    h('form', { ...props }, children);

  Form.Control = ({ as, ...props }: any) =>
    as === 'textarea' ? h('textarea', { ...props, role: 'textbox' }) : h('input', { ...props });

  const Dropdown = {
    Item: ({ children, ...props }: any) => h('button', { ...props }, children),
  };

  return {
    Container: ({ children, ...props }: any) => h('div', { ...props, 'data-testid': 'container' }, children),
    Table: ({ children, ...props }: any) => h('table', { ...props }, children),
    Modal,
    Button: ({ children, ...props }: any) => h('button', { ...props }, children),
    Form,
    Card,
    Row: ({ children, ...props }: any) => h('div', { ...props, className: 'row' }, children),
    Col: ({ children, ...props }: any) => h('div', { ...props, className: 'col' }, children),
    ButtonGroup: ({ children, ...props }: any) => h('div', { ...props }, children),
    Dropdown,
    DropdownButton: ({ children, title, ...props }: any) => h('button', { ...props }, title),
    Collapse: ({ children, in: inProp, ...props }: any) => inProp ? h('div', { ...props }, children) : null,
  };
});

// Mock react-feather icons
vi.mock('react-feather', () => ({
  ChevronDown: () => h('svg', { 'data-testid': 'chevron-down' }),
  ChevronUp: () => h('svg', { 'data-testid': 'chevron-up' }),
  Edit: () => h('svg', { 'data-testid': 'edit-icon' }),
  Monitor: () => h('svg', { 'data-testid': 'monitor-icon' }),
  Trash2: () => h('svg', { 'data-testid': 'trash-icon', className: 'feather-trash-2' }),
}));

// Mock i18next
vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string, options?: any) => {
      if (options && typeof options === 'object') {
        let result = key;
        Object.keys(options).forEach(optionKey => {
          result = result.replace(`{{${optionKey}}}`, options[optionKey]);
        });
        return result;
      }
      return key;
    },
  }),
}));

// Mock libsodium-wrappers
vi.mock('libsodium-wrappers', () => ({
  default: {
    crypto_box_seal_open: vi.fn(),
    crypto_box_seal: vi.fn(),
  },
}));

// Mock Base58
vi.mock('base58', () => ({
  int_to_base58: vi.fn((num: number) => `base58_${num}`),
}));

// Mock js-base64
vi.mock('js-base64', () => ({
  Base64: {
    toUint8Array: vi.fn(),
    fromUint8Array: vi.fn(),
  },
}));

// Mock utils
vi.mock('../utils', () => ({
  fetchClient: {
    GET: vi.fn(),
    POST: vi.fn(),
    DELETE: vi.fn(),
  },
  get_decrypted_secret: vi.fn(),
  pub_key: new Uint8Array(),
  secret: new Uint8Array(),
}));

// Mock Alert component
vi.mock('../components/Alert', () => ({
  showAlert: vi.fn(),
}));

// Mock preact-iso
vi.mock('preact-iso', () => ({
  useLocation: () => ({
    route: vi.fn(),
  }),
}));

// Mock median-js-bridge
vi.mock('median-js-bridge', () => ({
  default: {
    isNativeApp: () => false,
  },
}));

// Mock i18n
vi.mock('../i18n', () => ({
  default: {
    t: (key: string, options?: any) => {
      if (options && typeof options === 'object') {
        let result = key;
        Object.keys(options).forEach(optionKey => {
          result = result.replace(`{{${optionKey}}}`, options[optionKey]);
        });
        return result;
      }
      return key;
    },
  },
}));

// Mock Circle component
vi.mock('../components/Circle', () => ({
  Circle: vi.fn(() => null),
}));

beforeAll(() => {
  // Setup any global test configuration here
});
