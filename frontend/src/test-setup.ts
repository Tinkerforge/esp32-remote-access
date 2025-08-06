import '@testing-library/jest-dom';
import { beforeAll, vi } from 'vitest';
import { h, ComponentChildren } from 'preact';

interface MockComponentProps {
  children?: ComponentChildren;
  [key: string]: unknown;
}

interface ModalProps extends MockComponentProps {
  show?: boolean;
}

interface FormControlProps extends MockComponentProps {
  as?: string;
}

interface CollapseProps extends MockComponentProps {
  in?: boolean;
}

interface DropdownButtonProps extends MockComponentProps {
  title?: string;
}

// Mock react-bootstrap components with simple HTML elements
vi.mock('react-bootstrap', () => {
  const Form = ({ children, ...props }: MockComponentProps) =>
    h('form', { ...props, 'data-testid': 'form' }, children);

  Form.Group = ({ children, controlId, ...props }: MockComponentProps) => {
    if (children && Array.isArray(children)) {
      children = children.map((child) => {
        child.props = { ...child.props, controlId };
        return child;
      })
    }

    return h('div', { ...props, 'data-testid': 'form-group' }, children);
  }

  Form.Label = ({ children, controlId, ...props }: MockComponentProps) =>
    h('label', { ...props, htmlFor: controlId }, children);

  Form.Control = ({ onChange, value, isInvalid, type, as, controlId, ...props }: any) => {
    if (as === 'textarea') {
      return h('textarea', {
        ...props,
        role: 'textbox',
        value,
        onChange,
        id: controlId,
        className: isInvalid ? 'invalid' : '',
        'data-testid': `${type || 'textarea'}-input`
      });
    }
    return h('input', {
      ...props,
      type,
      value,
      onChange,
      id: controlId,
      className: isInvalid ? 'invalid' : '',
      'data-testid': `${type}-input`
    });
  };

  Form.Check = ({ checked, label, isInvalid, ...props }: any) =>
    h('div', {}, [
      h('input', {
        ...props,
        type: 'checkbox',
        checked,
        className: isInvalid ? 'invalid' : '',
        'data-testid': 'checkbox'
      }),
      h('label', {}, label)
    ]);

  (Form.Control as any).Feedback = ({ children, type, ...props }: any) =>
    h('div', { ...props, 'data-testid': `${type}-feedback` }, children);

  const Modal = ({ children, show, onHide, ...props }: any) =>
    show ? h('div', { ...props, 'data-testid': 'modal' }, [
      h('div', { className: 'modal-content' }, [
        children,
        h('button', { onClick: onHide, 'data-testid': 'modal-close' }, 'Close')
      ])
    ]) : null;

  Modal.Header = ({ children, closeButton, ...props }: any) =>
    h('div', { ...props, 'data-testid': 'modal-header' }, [
      children,
      closeButton && h('button', { 'data-testid': 'modal-close' }, 'x')
    ]);

  Modal.Title = ({ children, ...props }: MockComponentProps) =>
    h('h4', { ...props, 'data-testid': 'modal-title' }, children);

  Modal.Body = ({ children, ...props }: MockComponentProps) =>
    h('div', { ...props, 'data-testid': 'modal-body' }, children);

  Modal.Footer = ({ children, ...props }: MockComponentProps) =>
    h('div', { ...props, 'data-testid': 'modal-footer' }, children);

  const Card = ({ children, ...props }: MockComponentProps) =>
    h('div', { ...props, className: 'card' }, children);

  Card.Header = ({ children, ...props }: MockComponentProps) =>
    h('div', { ...props, className: 'card-header' }, children);
  Card.Body = ({ children, ...props }: MockComponentProps) =>
    h('div', { ...props, className: 'card-body' }, children);

  const Dropdown = {
    Item: ({ children, ...props }: MockComponentProps) => h('button', { ...props }, children),
  };

  const Nav = ({ children, ...props }: MockComponentProps) =>
    h('nav', { ...props }, children);

  Nav.Link = ({ children, ...props }: MockComponentProps) =>
    h('a', { ...props }, children);

  const Navbar = ({ children, ...props }: MockComponentProps) =>
    h('nav', { ...props, className: 'navbar' }, children);

  Navbar.Brand = ({ children, ...props }: MockComponentProps) =>
    h('a', { ...props, className: 'navbar-brand' }, children);

  Navbar.Toggle = ({ children, ...props }: MockComponentProps) =>
    h('button', { ...props, className: 'navbar-toggle' }, children);

  Navbar.Collapse = ({ children, ...props }: MockComponentProps) =>
    h('div', { ...props, className: 'navbar-collapse' }, children);

  const InputGroup = ({ children, ...props }: MockComponentProps) =>
    h('div', { ...props, className: 'input-group' }, children);

  InputGroup.Text = ({ children, ...props }: MockComponentProps) =>
    h('span', { ...props, className: 'input-group-text' }, children);

  const Tabs = ({ children, activeKey, onSelect, ...props }: any) =>
    h('div', { ...props, className: 'tabs' }, children);

  const Tab = ({ children, eventKey, title, ...props }: any) =>
    h('div', { ...props, className: 'tab-pane' }, children);

  const Alert = ({ children, variant, ...props }: any) =>
    h('div', { ...props, className: `alert alert-${variant || 'primary'}` }, children);

  const Spinner = ({ ...props }: MockComponentProps) =>
    h('div', { ...props, className: 'spinner' }, 'Loading...');

  return {
    Alert,
    Button: ({ children, type, ...props }: any) => h('button', { ...props, type, 'data-testid': 'submit-button' }, children),
    ButtonGroup: ({ children, ...props }: MockComponentProps) => h('div', { ...props }, children),
    Card,
    Col: ({ children, ...props }: MockComponentProps) => h('div', { ...props, className: 'col' }, children),
    Collapse: ({ children, in: inProp, ...props }: CollapseProps) => inProp ? h('div', { ...props }, children) : null,
    Container: ({ children, ...props }: MockComponentProps) => h('div', { ...props, 'data-testid': 'container' }, children),
    Dropdown,
    DropdownButton: ({ title, ...props }: DropdownButtonProps) => h('button', { ...props }, title),
    Form,
    InputGroup,
    Modal,
    Nav,
    Navbar,
    Row: ({ children, ...props }: MockComponentProps) => h('div', { ...props, className: 'row' }, children),
    Spinner,
    Tab,
    Table: ({ children, ...props }: MockComponentProps) => h('table', { ...props }, children),
    Tabs,
  };
});

// Mock react-feather icons
vi.mock('react-feather', () => ({
  ChevronDown: () => h('svg', { 'data-testid': 'chevron-down' }),
  ChevronUp: () => h('svg', { 'data-testid': 'chevron-up' }),
  Edit: () => h('svg', { 'data-testid': 'edit-icon' }),
  Eye: () => h('svg', { 'data-testid': 'eye-icon' }),
  Monitor: () => h('svg', { 'data-testid': 'monitor-icon' }),
  Trash2: () => h('svg', { 'data-testid': 'trash-icon', className: 'feather-trash-2' }),
}));

// Mock i18next
vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string, options?: Record<string, unknown>) => {
      if (options && typeof options === 'object') {
        let result = key;
        Object.keys(options).forEach(optionKey => {
          result = result.replace(`{{${optionKey}}}`, String(options[optionKey]));
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
    crypto_box_keypair: vi.fn(),
    crypto_secretbox_easy: vi.fn(),
    crypto_secretbox_NONCEBYTES: 24,
    crypto_secretbox_KEYBYTES: 32,
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
vi.mock('./i18n', () => ({
  default: {
    t: (key: string, options?: Record<string, unknown>) => {
      if (options && typeof options === 'object') {
        let result = key;
        Object.keys(options).forEach(optionKey => {
          result = result.replace(`{{${optionKey}}}`, String(options[optionKey]));
        });
        return result;
      }
      return key;
    },
    language: 'en',
  },
}));

// Mock Circle component
vi.mock('../components/Circle', () => ({
  Circle: vi.fn(() => null),
}));

beforeAll(() => {
  // Setup any global test configuration here
});
