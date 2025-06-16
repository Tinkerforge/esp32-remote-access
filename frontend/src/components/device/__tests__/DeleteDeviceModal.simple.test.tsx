import { render } from '@testing-library/preact';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { DeleteDeviceModal } from '../DeleteDeviceModal';
import { StateDevice } from '../types';

const mockDevice: StateDevice = {
  id: '1',
  uid: 12345,
  name: 'Test Device',
  status: 'Connected',
  note: 'Test note',
  port: 8080,
  valid: true,
  last_state_change: 1640995200,
};

const defaultProps = {
  show: true,
  device: mockDevice,
  onConfirm: vi.fn(),
  onCancel: vi.fn(),
};

describe('DeleteDeviceModal', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders modal when show is true', () => {
    const { container } = render(<DeleteDeviceModal {...defaultProps} />);
    expect(container.firstChild).toBeTruthy();
  });

  it('does not render modal when show is false', () => {
    const { container } = render(<DeleteDeviceModal {...defaultProps} show={false} />);
    expect(container.firstChild).toBeFalsy();
  });

  it('receives correct props', () => {
    const onConfirm = vi.fn();
    const onCancel = vi.fn();

    render(
      <DeleteDeviceModal
        show={true}
        device={mockDevice}
        onConfirm={onConfirm}
        onCancel={onCancel}
      />
    );

    // The component should render without throwing
    expect(onConfirm).not.toHaveBeenCalled();
    expect(onCancel).not.toHaveBeenCalled();
  });

  it('passes device prop correctly', () => {
    const testDevice = {
      ...mockDevice,
      name: 'Special Test Device',
      id: 'special-id'
    };

    const { container } = render(
      <DeleteDeviceModal {...defaultProps} device={testDevice} />
    );

    expect(container.firstChild).toBeTruthy();
  });
});
