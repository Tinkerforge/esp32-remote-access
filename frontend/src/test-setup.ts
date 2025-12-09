import '@testing-library/jest-dom';
import { beforeAll, vi } from 'vitest';
import { h, ComponentChildren } from 'preact';

interface MockComponentProps {
  children?: ComponentChildren;
  [key: string]: unknown;
}

interface CollapseProps extends MockComponentProps {
  in?: boolean;
}

interface DropdownButtonProps extends MockComponentProps {
  title?: string;
}

interface FormControlProps extends MockComponentProps {
  onChange?: (event: Event) => void;
  value?: string | number;
  isInvalid?: boolean;
  type?: string;
  as?: string;
  controlId?: string;
  disabled?: boolean;
  required?: boolean;
}

interface FormSelectProps extends MockComponentProps {
  onChange?: (event: Event) => void;
  value?: string | number;
  disabled?: boolean;
  controlId?: string;
}

interface FormCheckProps extends MockComponentProps {
  checked?: boolean;
  label?: string;
  isInvalid?: boolean;
  disabled?: boolean;
  id?: string;
  type?: string;
}

interface FormFeedbackProps extends MockComponentProps {
  children?: ComponentChildren;
  type?: string;
}

interface ModalProps extends MockComponentProps {
  show?: boolean;
  onHide?: () => void;
}

interface ModalHeaderProps extends MockComponentProps {
  closeButton?: boolean;
}

interface TabsProps extends MockComponentProps {
  activeKey?: string;
  onSelect?: (key: string | null) => void;
}

interface TabProps extends MockComponentProps {
  eventKey?: string;
  title?: string;
}

interface AlertProps extends MockComponentProps {
  variant?: string;
  dismissible?: boolean;
  onClose?: () => void;
}

interface ButtonProps extends MockComponentProps {
  type?: string;
  variant?: string;
  disabled?: boolean;
  onClick?: () => void;
}

// Mock react-bootstrap components with simple HTML elements
vi.mock('react-bootstrap', () => {
  const Form = ({ children, ...props }: MockComponentProps) =>
    h('form', { ...props, 'data-testid': 'form' }, children);

  Form.Group = ({ children, controlId, ...props }: MockComponentProps) => {
    if (children && Array.isArray(children)) {
      children = children.map((child) => {
        if (typeof child !== "object") {
          return child;
        }
        child.props = { ...child.props, controlId };
        return child;
      })
    }

    return h('div', { ...props, 'data-testid': 'form-group' }, children);
  }

  Form.Label = ({ children, controlId, ...props }: MockComponentProps) =>
    h('label', { ...props, htmlFor: controlId }, children);

  Form.Control = ({ onChange, value, isInvalid, type, as, controlId, disabled, required, ...props }: FormControlProps) => {
    if (as === 'textarea') {
      return h('textarea', {
        ...props,
        role: 'textbox',
        value,
        onChange,
        id: controlId,
        disabled,
        required,
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
      disabled,
      required,
      className: isInvalid ? 'invalid' : '',
      'data-testid': `${type}-input`
    });
  };

  Form.Select = ({ children, onChange, value, disabled, controlId, ...props }: FormSelectProps) =>
    h('select', {
      ...props,
      value,
      onChange,
      disabled,
      id: controlId,
    }, children);

  Form.Check = ({ checked, label, isInvalid, disabled, id, type, ...props }: FormCheckProps) => {
    return h('div', {}, [
      h('input', {
        ...props,
        type: type || 'checkbox',
        checked,
        disabled,
        id,
        className: isInvalid ? 'invalid' : '',
        'data-testid': 'checkbox'
      }),
      h('label', { htmlFor: id }, label)
    ]);
  };

  Form.Text = ({ children, ...props }: MockComponentProps) =>
    h('div', { ...props, 'data-testid': 'form-text' }, children);

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  (Form.Control as any).Feedback = ({ children, type, ...props }: FormFeedbackProps) =>
    h('div', { ...props, 'data-testid': `${type}-feedback` }, children);

  const Modal = ({ children, show, onHide, ...props }: ModalProps) =>
    show ? h('div', { ...props, 'data-testid': 'modal' }, [
      h('div', { className: 'modal-content' }, [
        children,
        h('button', { onClick: onHide, 'data-testid': 'modal-close' }, 'Close')
      ])
    ]) : null;

  Modal.Header = ({ children, closeButton, ...props }: ModalHeaderProps) =>
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

  const Nav = ({ children, className }: MockComponentProps) =>
    h('div', { className }, children);

  Nav.Link = ({ children, href, onClick, className }: MockComponentProps) =>
    h('a', { href, onClick, className }, children);

  const Navbar = ({ children, id, hidden, className }: MockComponentProps) =>
    h('nav', { id, hidden, className: className ? className : 'navbar', role: 'navigation' }, children);

  Navbar.Brand = ({ children, href, className }: MockComponentProps) =>
    h('a', { href, className: className ? className : 'navbar-brand' }, children);

  Navbar.Toggle = ({ children, onClick, id, 'aria-controls': ariaControls }: MockComponentProps & { 'aria-controls'?: string }) =>
    h('button', { onClick, id, 'aria-controls': ariaControls, className: 'navbar-toggle' }, children);

  Navbar.Collapse = ({ children, id, className }: MockComponentProps) =>
    h('div', { id, className: className ? className : 'navbar-collapse' }, children);

  const InputGroup = ({ children, ...props }: MockComponentProps) =>
    h('div', { ...props, className: 'input-group' }, children);

  InputGroup.Text = ({ children, ...props }: MockComponentProps) =>
    h('span', { ...props, className: 'input-group-text' }, children);

  const Tabs = ({ children, ...props }: TabsProps) =>
    h('div', { ...props, className: 'tabs' }, children);

  const Tab = ({ children, ...props }: TabProps) =>
    h('div', { ...props, className: 'tab-pane' }, children);

  const Alert = ({ children, variant, ...props }: AlertProps) =>
    h('div', { ...props, className: `alert alert-${variant || 'primary'}` }, [
      children,
      // Provide a close button when dismissible to simulate react-bootstrap behaviour
      props.dismissible && h('button', { 'data-testid': 'close-alert', onClick: props.onClose }, '×')
    ]);

  // Support <Alert.Heading> used by the component under test
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  (Alert as any).Heading = ({ children, ...props }: MockComponentProps) =>
    h('h4', { ...props, 'data-testid': 'alert-heading' }, children);

  const Spinner = ({ ...props }: MockComponentProps) =>
    h('div', { ...props, className: 'spinner' }, 'Loading...');

  const Badge = ({ children, bg, ...props }: MockComponentProps & { bg?: string }) =>
    h('span', { ...props, className: `badge bg-${bg || 'primary'}` }, children);

  const ListGroup = ({ children, ...props }: MockComponentProps) =>
    h('div', { ...props, className: 'list-group' }, children);

  ListGroup.Item = ({ children, active, action, onClick, ...props }: MockComponentProps & { active?: boolean; action?: boolean; onClick?: (e: Event) => void }) =>
    h('div', { ...props, className: `list-group-item${active ? ' active' : ''}${action ? ' action' : ''}`, onClick }, children);

  return {
    Alert,
    Badge,
    Button: ({ children, type, variant, disabled, onClick, ...props }: ButtonProps) => h('button', {
      ...props,
      type,
      disabled,
      onClick,
      className: variant ? `btn btn-${variant}` : 'btn',
      'data-testid': 'submit-button'
    }, children),
    ButtonGroup: ({ children, ...props }: MockComponentProps) => h('div', { ...props }, children),
    Card,
    Col: ({ children, ...props }: MockComponentProps) => h('div', { ...props, className: 'col' }, children),
    Collapse: ({ children, in: inProp, ...props }: CollapseProps) => inProp ? h('div', { ...props }, children) : null,
    Container: ({ children, ...props }: MockComponentProps) => h('div', { ...props, 'data-testid': 'container' }, children),
    Dropdown,
    DropdownButton: ({ title, ...props }: DropdownButtonProps) => h('button', { ...props }, title),
    Form,
    InputGroup,
    ListGroup,
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

// Separate mock for direct 'react-bootstrap/Alert' import used in Alert component
vi.mock('react-bootstrap/Alert', () => {
  interface MockAlertProps {
    children?: ComponentChildren;
    variant?: string;
    dismissible?: boolean;
    onClose?: () => void;
  }

  interface AlertComponent {
    (props: MockAlertProps): ReturnType<typeof h>;
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    Heading?: (props: { children?: ComponentChildren }) => any;
  }
  // Define without annotation first to prevent TS from narrowing attributes incompatibly
  const AlertFn = ({ children, variant, dismissible, onClose }: MockAlertProps) =>
    h('div', { className: `alert alert-${variant || 'primary'}` }, [
      children,
      dismissible && h('button', { 'data-testid': 'close-alert', onClick: onClose }, '×')
    ]);
  const Alert = AlertFn as AlertComponent;
  Alert.Heading = ({ children }: { children?: ComponentChildren }) => h('h4', { 'data-testid': 'alert-heading' }, children);
  return { default: Alert };
});

// Direct import path mock for Button (components import from 'react-bootstrap/Button')
vi.mock('react-bootstrap/Button', () => {
  return {
    default: ({ children, type, disabled, onClick }: { children?: ComponentChildren; type?: string; disabled?: boolean; onClick?: () => void }) =>
      h('button', { type, disabled, onClick, 'data-testid': 'submit-button' }, children),
  };
});

// Direct import path mock for Form (used as default import in some tests)
vi.mock('react-bootstrap/Form', () => {
  const Form = ({ children, onSubmit }: { children?: ComponentChildren; onSubmit?: (e: Event) => void }) =>
    h('form', { onSubmit }, children);
  Form.Group = ({ children, controlId }: { children?: ComponentChildren; controlId?: string }) => {
    if (children && Array.isArray(children)) {
      children = children.map((child) => {
        if (typeof child !== 'object') return child;
        child.props = { ...child.props, controlId };
        return child;
      });
    }
    return h('div', {}, children);
  };
  Form.Label = ({ children, controlId }: { children?: ComponentChildren; controlId?: string }) =>
    h('label', { htmlFor: controlId }, children);
  Form.Control = ({
    type,
    value,
    onChange,
    isInvalid,
    controlId,
  }: {
    type: string;
    value?: string;
    onChange?: (e: Event) => void;
    isInvalid?: boolean;
    controlId?: string;
  }) =>
    h('input', {
      id: controlId,
      type,
      value,
      onChange,
      'data-testid': `${type}-input`,
      className: isInvalid ? 'invalid' : '',
    });
  return { default: Form };
});

// Direct import path mocks for Nav and Navbar
vi.mock('react-bootstrap/Nav', () => {
  const Nav = ({ children, className }: MockComponentProps) => h('div', { className }, children);
  const NavWithStatics = Object.assign(Nav, {
    Link: ({ children, href, onClick, className }: MockComponentProps) =>
      h('a', { href, onClick, className }, children),
  });
  return { default: NavWithStatics };
});

vi.mock('react-bootstrap/Navbar', () => {
  const Navbar = ({ children, id, hidden, className }: MockComponentProps) =>
    h('nav', { id, hidden, className, role: 'navigation' }, children);
  const NavbarWithStatics = Object.assign(Navbar, {
    Brand: ({ children, href, className }: MockComponentProps) => h('a', { href, className }, children),
    Toggle: ({ children, onClick, id, 'aria-controls': ariaControls }: MockComponentProps & { 'aria-controls'?: string }) =>
      h('button', { onClick, id, 'aria-controls': ariaControls, className: 'navbar-toggle' }, children),
    Collapse: ({ children, id, className }: MockComponentProps) => h('div', { id, className }, children),
  });
  return { default: NavbarWithStatics };
});

// Mock react-feather icons (use non-SVG to avoid namespace issues in jsdom)
vi.mock('react-feather', () => ({
  ChevronDown: () => h('span', { 'data-testid': 'chevron-down' }),
  ChevronUp: () => h('span', { 'data-testid': 'chevron-up' }),
  Edit: () => h('span', { 'data-testid': 'edit-icon' }),
  Edit2: () => h('span', { 'data-testid': 'edit2-icon' }),
  Clipboard: () => h('span', { 'data-testid': 'clipboard-icon' }),
  Eye: () => h('span', { 'data-testid': 'eye-icon' }),
  EyeOff: () => h('span', { 'data-testid': 'eye-off-icon' }),
  Monitor: () => h('span', { 'data-testid': 'monitor-icon' }),
  Plus: () => h('span', { 'data-testid': 'plus-icon' }),
  Trash2: () => h('span', { 'data-testid': 'trash-icon', className: 'feather-trash-2' }),
  Key: () => h('span', { 'data-testid': 'key-icon' }),
  LogOut: () => h('span', { 'data-testid': 'logout-icon' }),
  Server: () => h('span', { 'data-testid': 'server-icon' }),
  User: () => h('span', { 'data-testid': 'user-icon' }),
  Search: () => h('span', { 'data-testid': 'search-icon' }),
  X: () => h('span', { 'data-testid': 'x-icon', style: { cursor: 'pointer' } }),
}));

// Mock i18next
vi.mock('react-i18next', () => {
  const t = (key: string, options?: Record<string, unknown>) => {
    if (options && typeof options === 'object') {
      let result = key;
      Object.keys(options).forEach(optionKey => {
        result = result.replace(`{{${optionKey}}}`, String(options[optionKey]));
      });
      return result;
    }
    return key;
  };

  return {
    useTranslation: () => ({
      t,
    }),
    Trans: ({ children }: { children?: ComponentChildren }) => h('span', {}, children),
  };
});

// Mock libsodium-wrappers
vi.mock('libsodium-wrappers', () => ({
  default: {
  // Provide a resolved ready promise to mirror the real libsodium API
  ready: Promise.resolve(),
    crypto_box_seal_open: vi.fn(),
    crypto_box_seal: vi.fn(),
    crypto_box_keypair: vi.fn(),
    crypto_secretbox_easy: vi.fn(),
    crypto_secretbox_open_easy: vi.fn(),
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
  // Provide deterministic return; tests can override with mockReturnValueOnce
  fromUint8Array: vi.fn(() => 'encoded'),
  },
}));

// Mock utils
vi.mock('./utils', () => ({
  fetchClient: {
    GET: vi.fn(),
    POST: vi.fn(),
    PUT: vi.fn(),
    DELETE: vi.fn(),
  },
  get_decrypted_secret: vi.fn(),
  pub_key: new Uint8Array(),
  secret: new Uint8Array(),
  generate_hash: vi.fn(),
  generate_random_bytes: vi.fn(),
  get_salt: vi.fn(),
  get_salt_for_user: vi.fn(),
  concat_salts: vi.fn(),
  // App/login related state & helpers for Login component tests
  AppState: { Loading: 0, LoggedIn: 1, LoggedOut: 2, Recovery: 3 },
  loggedIn: { value: 0 },
  storeSecretKeyInServiceWorker: vi.fn(),
  bc: { postMessage: vi.fn() },
  isDebugMode: { value: false },
  resetSecret: vi.fn(),
  clearSecretKeyFromServiceWorker: vi.fn().mockResolvedValue(undefined),
  FRONTEND_URL: '',
  refresh_access_token: vi.fn(),
  startVersionChecking: vi.fn(),
}));

// Some files import from './utils.js' (with extension) — provide identical mock for that path
vi.mock('./utils.js', () => ({
  fetchClient: {
    GET: vi.fn(),
    POST: vi.fn(),
    PUT: vi.fn(),
    DELETE: vi.fn(),
  },
  get_decrypted_secret: vi.fn(),
  pub_key: new Uint8Array(),
  secret: new Uint8Array(),
  generate_hash: vi.fn(),
  generate_random_bytes: vi.fn(),
  get_salt: vi.fn(),
  get_salt_for_user: vi.fn(),
  concat_salts: vi.fn(),
  AppState: { Loading: 0, LoggedIn: 1, LoggedOut: 2, Recovery: 3 },
  loggedIn: { value: 0 },
  storeSecretKeyInServiceWorker: vi.fn(),
  bc: { postMessage: vi.fn() },
  isDebugMode: { value: false },
  resetSecret: vi.fn(),
  clearSecretKeyFromServiceWorker: vi.fn().mockResolvedValue(undefined),
  FRONTEND_URL: '',
  refresh_access_token: vi.fn(),
  startVersionChecking: vi.fn(),
}));

// Assets aliases used in index.tsx
vi.mock('logo', () => ({ default: 'logo.png' }));
vi.mock('favicon', () => ({ default: '/favicon.png' }));

// Mock preact-iso
vi.mock('preact-iso', () => ({
  useLocation: () => ({
    route: vi.fn(),
    url: '/',
  }),
  useRoute: () => ({ params: {} }),
  LocationProvider: ({ children }: { children?: ComponentChildren }) => h('div', {}, children),
  Router: ({ children }: { children?: ComponentChildren }) => h('div', {}, children),
  Route: ({ children }: { children?: ComponentChildren }) => h('div', {}, children),
  lazy: () => (props: Record<string, unknown>) => h('div', { 'data-testid': 'lazy-component', ...props }),
}));

// Mock median-js-bridge
vi.mock('median-js-bridge', () => ({
  default: {
    isNativeApp: () => false,
    sidebar: { setItems: vi.fn() },
  share: { downloadFile: vi.fn() },
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
vi.mock('./components/Circle', () => ({
  Circle: vi.fn(() => null),
}));

// Mock components/Navbar - keep all exports real for testing
// Tests will mock specific functions as needed
vi.mock('./components/Navbar', async (importOriginal) => {
  const actual = await importOriginal<typeof import('./components/Navbar')>();
  return {
    ...actual,
  };
});

// Also mock direct Alert import path used by components (Navbar imports './Alert')
vi.mock('./Alert', () => ({
  showAlert: vi.fn(),
}));

// Mock PasswordComponent
vi.mock('./components/PasswordComponent', () => ({
  PasswordComponent: ({ onChange, isInvalid, invalidMessage, controlId }: {
    onChange: (value: string) => void;
    isInvalid: boolean;
    invalidMessage: string;
    controlId: string;
  }) => {
    return h('div', {}, [
      h('input', {
        type: 'textbox',
        'data-testid': 'password-input',
        onChange: (e: Event) => onChange((e.target as HTMLInputElement).value),
        id: controlId,
        className: isInvalid ? 'invalid' : '',
      }),
      isInvalid && h('div', { 'data-testid': 'password-error' }, invalidMessage)
    ]);
  },
}));

// Mock @preact/signals
vi.mock('@preact/signals', async (useOriginal) => {
  const original = await useOriginal();
  return {
    ...(typeof original === 'object' && original !== null ? original : {}),
    signal: (value: unknown) => ({
      value,
    }),
  }
});

beforeAll(() => {
  // Setup any global test configuration here

  // Mock localStorage
  const store = new Map<string, string>();
  const localStorageMock = {
    getItem: vi.fn((key: string) => {
      const val = store.get(key);
      return typeof val === 'undefined' ? null : val;
    }),
    setItem: vi.fn((key: string, value: string) => { store.set(key, String(value)); }),
    removeItem: vi.fn((key: string) => { store.delete(key); }),
    clear: vi.fn(() => { store.clear(); }),
    key: vi.fn((index: number) => Array.from(store.keys())[index] ?? null),
  } as unknown as Storage;
  Object.defineProperty(localStorageMock, 'length', { get: () => store.size });
  Object.defineProperty(window, 'localStorage', {
    value: localStorageMock,
    configurable: true,
    writable: true,
  });

  // Mock window.location.reload
  Object.defineProperty(window, 'location', {
    value: {
      reload: vi.fn(),
      replace: vi.fn(),
      href: 'http://localhost:3000',
    },
    writable: true,
  });

  window.scrollTo = vi.fn();

  // Provide minimal Web Crypto mock used by components
  if (!('crypto' in window)) {
    // @ts-expect-error - define minimal crypto
    window.crypto = {};
  }
  if (!('subtle' in window.crypto)) {
    // @ts-expect-error - define minimal subtle
    window.crypto.subtle = { digest: vi.fn().mockResolvedValue(new ArrayBuffer(0)) };
  }

  // URL.createObjectURL / revokeObjectURL in jsdom
  if (!('createObjectURL' in URL)) {
    // @ts-expect-error - add createObjectURL
    URL.createObjectURL = vi.fn(() => 'blob:mock');
  }
  if (!('revokeObjectURL' in URL)) {
    // @ts-expect-error - add revokeObjectURL
    URL.revokeObjectURL = vi.fn();
  }

  // File polyfill for environments missing it
  if (typeof (globalThis as unknown as { File?: unknown }).File === 'undefined') {
    class PolyfillFile extends Blob {
      name: string;
      lastModified: number;
      constructor(bits: BlobPart[], name: string, options?: FilePropertyBag) {
        super(bits, options);
        this.name = name;
        this.lastModified = Date.now();
      }
    }
    (globalThis as unknown as { File: unknown }).File = PolyfillFile;
  }
});

// Provide a consistent mock for modules importing './components/Alert'
vi.mock('./components/Alert', () => ({
  showAlert: vi.fn(),
  ErrorAlert: () => h('div', { 'data-testid': 'error-alert' }),
}));

// Direct import path mocks for react-bootstrap components used by index.tsx
vi.mock('react-bootstrap/Row', () => ({
  default: ({ children, ...props }: MockComponentProps) => h('div', { ...props, className: 'row' }, children),
}));
vi.mock('react-bootstrap/Card', () => ({
  default: ({ children, ...props }: MockComponentProps) => h('div', { ...props, className: 'card' }, children),
}));
vi.mock('react-bootstrap/Tabs', () => ({
  default: ({ children, ...props }: MockComponentProps) => h('div', { ...props, className: 'tabs' }, children),
}));
vi.mock('react-bootstrap/Tab', () => ({
  default: ({ children, ...props }: MockComponentProps) => h('div', { ...props, className: 'tab-pane' }, children),
}));
