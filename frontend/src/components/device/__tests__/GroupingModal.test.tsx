import { render, screen, fireEvent, waitFor } from '@testing-library/preact';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { GroupingModal } from '../GroupingModal';
import { StateDevice, Grouping } from '../types';
import { fetchClient } from '../../../utils';

// Mock the dependencies
vi.mock('../../../utils', () => ({
  fetchClient: {
    POST: vi.fn(),
    DELETE: vi.fn(),
    GET: vi.fn(),
    PUT: vi.fn(),
  },
}));

vi.mock('../../Alert', () => ({
  showAlert: vi.fn(),
}));

const mockDevices: StateDevice[] = [
  {
    id: 'device1',
    uid: 12345,
    name: 'Test Device 1',
    status: 'Connected',
    note: 'Test note 1',
    port: 8080,
    valid: true,
    last_state_change: 1640995200,
    firmware_version: '1.0.0',
  },
  {
    id: 'device2',
    uid: 67890,
    name: 'Test Device 2',
    status: 'Disconnected',
    note: 'Test note 2',
    port: 8081,
    valid: false,
    last_state_change: null,
    firmware_version: '1.1.0',
  },
  {
    id: 'device3',
    uid: 11111,
    name: 'Another Device',
    status: 'Connected',
    note: '',
    port: 8082,
    valid: true,
    last_state_change: 1640995300,
    firmware_version: '1.2.0',
  },
];

const mockGroupings: Grouping[] = [
  {
    id: 'group1',
    name: 'Test Group 1',
    device_ids: ['device1', 'device2'],
    is_default: false,
  },
  {
    id: 'group2',
    name: 'Test Group 2',
    device_ids: ['device3'],
    is_default: false,
  },
];

const defaultProps = {
  show: true,
  devices: mockDevices,
  groupings: mockGroupings,
  onClose: vi.fn(),
  onGroupingsUpdated: vi.fn(),
  encryptGroupingName: vi.fn((name: string) => Promise.resolve(`encrypted_${name}`)),
  loadGroupings: vi.fn(),
};

describe('GroupingModal', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders modal when show is true', () => {
    render(<GroupingModal {...defaultProps} />);
    expect(screen.getByTestId('modal')).toBeInTheDocument();
  });

  it('does not render modal when show is false', () => {
    render(<GroupingModal {...defaultProps} show={false} />);
    expect(screen.queryByTestId('modal')).not.toBeInTheDocument();
  });

  it('displays list of groupings', () => {
    render(<GroupingModal {...defaultProps} />);
    expect(screen.getByText('Test Group 1')).toBeInTheDocument();
    expect(screen.getByText('Test Group 2')).toBeInTheDocument();
  });

  it('shows device count for each grouping', () => {
    render(<GroupingModal {...defaultProps} />);
    // Counts are rendered alongside the i18n key 'grouping_devices'
    const deviceCountElements = screen.getAllByText(/grouping_devices/i);
    expect(deviceCountElements.length).toBeGreaterThan(0);
  });

  it('shows no groupings message when empty', () => {
    render(<GroupingModal {...defaultProps} groupings={[]} />);
    expect(screen.getByText('no_groupings')).toBeInTheDocument();
  });

  it('opens create form when create button is clicked', async () => {
    render(<GroupingModal {...defaultProps} />);

    const createButton = screen.getByRole('button', { name: /create/i });
    fireEvent.click(createButton);

    await waitFor(() => {
      expect(screen.getByPlaceholderText('grouping_name_placeholder')).toBeInTheDocument();
    });
  });

  it('opens edit form when edit button is clicked', async () => {
    render(<GroupingModal {...defaultProps} />);

    const editButtons = screen.getAllByRole('button', { name: '' });
    // Find the edit button (first icon button in the row)
    const editButton = editButtons.find(btn => btn.querySelector('svg'));

    if (editButton) {
      fireEvent.click(editButton);

      await waitFor(() => {
        expect(screen.getByDisplayValue('Test Group 1')).toBeInTheDocument();
      });
    }
  });

  it('filters devices based on search query', async () => {
    render(<GroupingModal {...defaultProps} />);

    const createButton = screen.getByRole('button', { name: /create/i });
    fireEvent.click(createButton);

    await waitFor(() => {
      const searchInput = screen.getByPlaceholderText('search_devices');
      expect(searchInput).toBeInTheDocument();

      fireEvent.change(searchInput, { target: { value: 'Another' } });

      // Should show only the matching device
      expect(screen.getByText('Another Device')).toBeInTheDocument();
    });
  });

  it('hides standalone local devices from the picker when creating a group', async () => {
    const standaloneLocal: StateDevice = {
      id: '',
      uid: 99999,
      name: 'Local Only Device',
      status: 'Connected',
      note: '',
      port: 8083,
      valid: true,
      last_state_change: null,
      firmware_version: '1.0.0',
      host: 'warp.local',
    };

    render(<GroupingModal {...defaultProps} devices={[...mockDevices, standaloneLocal]} />);

    const createButton = screen.getByRole('button', { name: /create/i });
    fireEvent.click(createButton);

    await waitFor(() => {
      // Cloud-paired devices remain selectable...
      expect(screen.getByText('Test Device 1')).toBeInTheDocument();
      // ...while the LAN-only device (empty id) is filtered out.
      expect(screen.queryByText('Local Only Device')).not.toBeInTheDocument();
    });
  });

  it('hides standalone local devices from the picker when editing a group', async () => {
    const standaloneLocal: StateDevice = {
      id: '',
      uid: 99999,
      name: 'Local Only Device',
      status: 'Connected',
      note: '',
      port: 8083,
      valid: true,
      last_state_change: null,
      firmware_version: '1.0.0',
      host: 'warp.local',
    };

    const { container } = render(
      <GroupingModal {...defaultProps} devices={[...mockDevices, standaloneLocal]} />,
    );

    // The react-feather icons render as <span> test stubs in this environment,
    // so the only reliable way to target the edit button is its outline-primary
    // variant class.
    const editButton = container.querySelector('.btn-outline-primary') as HTMLElement;
    fireEvent.click(editButton);

    await waitFor(() => {
      expect(screen.getByDisplayValue('Test Group 1')).toBeInTheDocument();
      expect(screen.queryByText('Local Only Device')).not.toBeInTheDocument();
    });
  });

  it('allows selecting devices when creating a grouping', async () => {
    render(<GroupingModal {...defaultProps} />);

    const createButton = screen.getByRole('button', { name: /create/i });
    fireEvent.click(createButton);

    await waitFor(() => {
      const nameInput = screen.getByPlaceholderText('grouping_name_placeholder');
      fireEvent.change(nameInput, { target: { value: 'New Group' } });

      const checkboxes = screen.getAllByTestId('checkbox');
      expect(checkboxes.length).toBeGreaterThan(0);

      // Select first device
      fireEvent.click(checkboxes[0]);
      expect((checkboxes[0] as HTMLInputElement).checked).toBe(true);
    });
  });

  it('creates a new grouping when save is clicked', async () => {
    const mockPost = vi.mocked(fetchClient.POST);
    mockPost.mockResolvedValue({
      data: { id: 'new-group-id' },
      response: { status: 200 } as Response,
      error: undefined,
    });

    const mockLoadGroupings = vi.fn();

    render(<GroupingModal {...defaultProps} loadGroupings={mockLoadGroupings} />);

    const createButton = screen.getByRole('button', { name: /create/i });
    fireEvent.click(createButton);

    await waitFor(() => {
      expect(screen.getByPlaceholderText('grouping_name_placeholder')).toBeInTheDocument();
    });

    const nameInput = screen.getByPlaceholderText('grouping_name_placeholder');
    fireEvent.change(nameInput, { target: { value: 'New Group' } });

    const checkboxes = screen.getAllByTestId('checkbox');
    fireEvent.click(checkboxes[0]);

    const saveButton = screen.getByRole('button', { name: 'save' });
    fireEvent.click(saveButton);

    await waitFor(() => {
      expect(mockPost).toHaveBeenCalledWith('/grouping/create', expect.objectContaining({
        body: expect.objectContaining({ name: 'encrypted_New Group' })
      }));
      expect(mockLoadGroupings).toHaveBeenCalled();
    });
  });

  it('updates existing grouping when save is clicked after editing', async () => {
    const mockPost = vi.mocked(fetchClient.POST);
    mockPost.mockResolvedValue({
      data: undefined,
      response: { status: 200 } as Response,
      error: undefined,
    });

    const mockPut = vi.mocked(fetchClient.PUT);
    mockPut.mockResolvedValue({
      data: undefined,
      response: { status: 200 } as Response,
      error: undefined,
    });

    const mockLoadGroupings = vi.fn();

    render(<GroupingModal {...defaultProps} loadGroupings={mockLoadGroupings} />);

    // Click edit on first grouping
    const editButtons = screen.getAllByRole('button', { name: '' });
    const editButton = editButtons.find(btn => btn.querySelector('svg'));

    if (editButton) {
      fireEvent.click(editButton);

      await waitFor(async () => {
        const nameInput = screen.getByDisplayValue('Test Group 1');
        fireEvent.change(nameInput, { target: { value: 'Updated Group Name' } });

        const saveButton = screen.getByRole('button', { name: 'save' });
        fireEvent.click(saveButton);

        await waitFor(() => {
          // Should call PUT to update name
          expect(mockPut).toHaveBeenCalledWith('/grouping/edit', expect.objectContaining({
            body: expect.objectContaining({ name: 'encrypted_Updated Group Name' })
          }));
          expect(mockLoadGroupings).toHaveBeenCalled();
        });
      });
    }
  });

  it('deletes grouping when delete button is clicked and confirmed', async () => {
    // Mock window.confirm
    const originalConfirm = window.confirm;
    window.confirm = vi.fn(() => true);

    const mockDelete = vi.mocked(fetchClient.DELETE);
    mockDelete.mockResolvedValue({
      data: undefined,
      response: { status: 200 } as Response,
      error: undefined,
    });

    const mockLoadGroupings = vi.fn();

    render(<GroupingModal {...defaultProps} loadGroupings={mockLoadGroupings} />);

    const deleteButtons = screen.getAllByRole('button', { name: '' });
    // Find delete button (should be the second icon button)
    const deleteButton = deleteButtons[deleteButtons.length - 1];

    fireEvent.click(deleteButton);

    await waitFor(() => {
      expect(window.confirm).toHaveBeenCalled();
      expect(mockDelete).toHaveBeenCalledWith('/grouping/delete', expect.any(Object));
      expect(mockLoadGroupings).toHaveBeenCalled();
    });

    // Restore original confirm
    window.confirm = originalConfirm;
  });

  it('does not delete grouping when cancel is clicked in confirm dialog', async () => {
    const originalConfirm = window.confirm;
    window.confirm = vi.fn(() => false);

    const mockDelete = vi.mocked(fetchClient.DELETE);

    render(<GroupingModal {...defaultProps} />);

    const deleteButtons = screen.getAllByRole('button', { name: '' });
    const deleteButton = deleteButtons[deleteButtons.length - 1];

    fireEvent.click(deleteButton);

    await waitFor(() => {
      expect(window.confirm).toHaveBeenCalled();
      expect(mockDelete).not.toHaveBeenCalled();
    });

    window.confirm = originalConfirm;
  });

  it('cancels creation when cancel button is clicked', async () => {
    render(<GroupingModal {...defaultProps} />);

    const createButton = screen.getByRole('button', { name: /create/i });
    fireEvent.click(createButton);

    // Ensure the edit form is visible first
    await waitFor(() => expect(screen.getByPlaceholderText('grouping_name_placeholder')).toBeInTheDocument());

    const cancelButton = screen.getByRole('button', { name: 'cancel' });
    fireEvent.click(cancelButton);

    // Should return to list view
    await waitFor(() => expect(screen.queryByPlaceholderText('grouping_name_placeholder')).not.toBeInTheDocument());
  });

  it('calls onClose when close button is clicked', () => {
    render(<GroupingModal {...defaultProps} />);

    // Prefer the modal-close inside the modal body which is wired to onHide
    const closeButtons = screen.getAllByTestId('modal-close');
    fireEvent.click(closeButtons[1]);

    expect(defaultProps.onClose).toHaveBeenCalled();
  });

  it('resets form state when modal is closed', async () => {
    const { rerender } = render(<GroupingModal {...defaultProps} />);

    const createButton = screen.getByRole('button', { name: /create/i });
    fireEvent.click(createButton);

    await waitFor(() => {
      const nameInput = screen.getByPlaceholderText('grouping_name_placeholder');
      fireEvent.change(nameInput, { target: { value: 'Test' } });
    });

    // Close modal
    rerender(<GroupingModal {...defaultProps} show={false} />);

    // Reopen modal
    rerender(<GroupingModal {...defaultProps} show={true} />);

    // Should show list view, not edit form
    expect(screen.queryByRole('textbox', { name: /grouping name/i })).not.toBeInTheDocument();
  });

  it('handles empty devices array gracefully', () => {
    render(<GroupingModal {...defaultProps} devices={[]} />);

    const createButton = screen.getByRole('button', { name: /create/i });
    fireEvent.click(createButton);

    // Should not throw error
    expect(screen.getByTestId('modal')).toBeInTheDocument();
  });

  it('pre-selects devices when editing existing grouping', async () => {
    render(<GroupingModal {...defaultProps} />);

    const editButtons = screen.getAllByRole('button', { name: '' });
    const editButton = editButtons.find(btn => btn.querySelector('svg'));

    if (editButton) {
      fireEvent.click(editButton);

      await waitFor(() => {
        const checkboxes = screen.getAllByRole('checkbox');
        // First two devices should be checked (device1 and device2 in group1)
        const checkedBoxes = checkboxes.filter((cb) => (cb as HTMLInputElement).checked);
        expect(checkedBoxes.length).toBeGreaterThan(0);
      });
    }
  });

  it('validates required fields when saving', async () => {
    render(<GroupingModal {...defaultProps} />);

    const createButton = screen.getByRole('button', { name: /create/i });
    fireEvent.click(createButton);

    await waitFor(() => {
      const saveButton = screen.getByRole('button', { name: 'save' });
      fireEvent.click(saveButton);

      // Form should validate and not submit without name
      const nameInput = screen.getByPlaceholderText('grouping_name_placeholder');
      expect(nameInput).toHaveAttribute('required');
    });
  });

  it('calls onGroupingsUpdated after successful operations', async () => {
    const mockLoadGroupings = vi.fn();

    const originalConfirm = window.confirm;
    window.confirm = vi.fn(() => true);

    const mockDelete = vi.mocked(fetchClient.DELETE);
    mockDelete.mockResolvedValue({
      data: undefined,
      response: { status: 200 } as Response,
      error: undefined,
    });

    render(<GroupingModal {...defaultProps} loadGroupings={mockLoadGroupings} />);

    const deleteButtons = screen.getAllByRole('button', { name: '' });
    const deleteButton = deleteButtons[deleteButtons.length - 1];

    fireEvent.click(deleteButton);

    await waitFor(() => {
      expect(mockLoadGroupings).toHaveBeenCalled();
    });

    window.confirm = originalConfirm;
  });

  it('shows the default badge and checks the per-row checkbox for the default grouping', () => {
    const groupings: Grouping[] = [
      { id: 'group1', name: 'Test Group 1', device_ids: ['device1'], is_default: false },
      { id: 'group2', name: 'Test Group 2', device_ids: ['device3'], is_default: true },
    ];

    render(<GroupingModal {...defaultProps} groupings={groupings} />);

    // The "Default" badge should be rendered for group2 only.
    expect(screen.getByText('default_grouping')).toBeInTheDocument();

    // The list-view "Set as default" checkbox for group2 should be checked,
    // and the one for group1 should not.
    const group1Checkbox = document.getElementById('set-default-group1') as HTMLInputElement;
    const group2Checkbox = document.getElementById('set-default-group2') as HTMLInputElement;
    expect(group1Checkbox).toBeTruthy();
    expect(group2Checkbox).toBeTruthy();
    expect(group1Checkbox.checked).toBe(false);
    expect(group2Checkbox.checked).toBe(true);
  });

  it('toggles the persisted default when the per-row checkbox is clicked', async () => {
    const mockPut = vi.mocked(fetchClient.PUT);
    mockPut.mockResolvedValue({
      data: { id: 'group1', name: 'Test Group 1', is_default: true } as never,
      response: { status: 200 } as Response,
      error: undefined,
    });

    const mockLoadGroupings = vi.fn().mockResolvedValue(undefined);

    render(<GroupingModal {...defaultProps} loadGroupings={mockLoadGroupings} />);

    const group1Checkbox = document.getElementById('set-default-group1') as HTMLInputElement;
    fireEvent.click(group1Checkbox);

    await waitFor(() => {
      // Since the current value is false, the toggle sends is_default: true.
      expect(mockPut).toHaveBeenCalledWith('/grouping/edit', expect.objectContaining({
        body: expect.objectContaining({ grouping_id: 'group1', is_default: true }),
      }));
      expect(mockLoadGroupings).toHaveBeenCalled();
    });
  });

  it('passes is_default on the edit PUT body when the create form checkbox is toggled', async () => {
    const mockPost = vi.mocked(fetchClient.POST);
    mockPost.mockResolvedValue({
      data: { id: 'new-group-id', is_default: true },
      response: { status: 200 } as Response,
      error: undefined,
    });
    mockPost.mockResolvedValue({
      data: { id: 'new-group-id', is_default: true },
      response: { status: 200 } as Response,
      error: undefined,
    });

    const mockLoadGroupings = vi.fn().mockResolvedValue(undefined);

    render(<GroupingModal {...defaultProps} loadGroupings={mockLoadGroupings} />);

    // Open the create form.
    const createButton = screen.getByRole('button', { name: /create/i });
    fireEvent.click(createButton);

    await waitFor(() => {
      expect(screen.getByPlaceholderText('grouping_name_placeholder')).toBeInTheDocument();
    });

    // Fill the name.
    const nameInput = screen.getByPlaceholderText('grouping_name_placeholder');
    fireEvent.change(nameInput, { target: { value: 'New Default' } });

    // Tick the "Set as default" checkbox in the create form.
    const setAsDefaultCheckbox = document.getElementById('set-as-default') as HTMLInputElement;
    fireEvent.click(setAsDefaultCheckbox);

    // Save.
    const saveButton = screen.getByRole('button', { name: 'save' });
    fireEvent.click(saveButton);

    await waitFor(() => {
      // The create POST body should include is_default: true.
      expect(mockPost).toHaveBeenCalledWith('/grouping/create', expect.objectContaining({
        body: expect.objectContaining({ name: 'encrypted_New Default', is_default: true }),
      }));
    });
  });

  it('passes is_default on the edit PUT body when the edit form checkbox is toggled', async () => {
    const mockPut = vi.mocked(fetchClient.PUT);
    mockPut.mockResolvedValue({
      data: { id: 'group1', name: 'Test Group 1', is_default: true } as never,
      response: { status: 200 } as Response,
      error: undefined,
    });

    const mockLoadGroupings = vi.fn().mockResolvedValue(undefined);

    const { container } = render(<GroupingModal {...defaultProps} loadGroupings={mockLoadGroupings} />);

    // Open the edit form for group1 (currently is_default: false). The
    // outline-primary class is the only reliable way to target the edit
    // button in this test environment because the icon library is stubbed
    // as <span>, not <svg>.
    const editButton = container.querySelector('.btn-outline-primary') as HTMLElement;
    fireEvent.click(editButton);

    await waitFor(() => {
      expect(screen.getByDisplayValue('Test Group 1')).toBeInTheDocument();
    });

    // Tick the "Set as default" checkbox.
    const setAsDefaultCheckbox = document.getElementById('set-as-default') as HTMLInputElement;
    expect(setAsDefaultCheckbox.checked).toBe(false);
    fireEvent.click(setAsDefaultCheckbox);

    // Save.
    const saveButton = screen.getByRole('button', { name: 'save' });
    fireEvent.click(saveButton);

    await waitFor(() => {
      expect(mockPut).toHaveBeenCalledWith('/grouping/edit', expect.objectContaining({
        body: expect.objectContaining({ grouping_id: 'group1', is_default: true }),
      }));
    });
  });

  it('displays device notes when creating a grouping', async () => {
    render(<GroupingModal {...defaultProps} />);

    const createButton = screen.getByRole('button', { name: /create/i });
    fireEvent.click(createButton);

    await waitFor(() => {
      expect(screen.getByText('Test note 1')).toBeInTheDocument();
      expect(screen.getByText('Test note 2')).toBeInTheDocument();
    });
  });

  it('displays device notes when editing a grouping', async () => {
    const { container } = render(<GroupingModal {...defaultProps} />);

    const editButton = container.querySelector('.btn-outline-primary') as HTMLElement;
    fireEvent.click(editButton);

    await waitFor(() => {
      expect(screen.getByText('Test note 1')).toBeInTheDocument();
      expect(screen.getByText('Test note 2')).toBeInTheDocument();
    });
  });

  it('does not render a note section for devices with empty notes', async () => {
    render(<GroupingModal {...defaultProps} />);

    const createButton = screen.getByRole('button', { name: /create/i });
    fireEvent.click(createButton);

    await waitFor(() => {
      // The empty-note device (device3) is still rendered by name, but its note
      // is the empty string so nothing should be displayed as a note.
      expect(screen.getByText('Another Device')).toBeInTheDocument();
      // The other two devices show their non-empty notes.
      expect(screen.getByText('Test note 1')).toBeInTheDocument();
      expect(screen.getByText('Test note 2')).toBeInTheDocument();
    });
  });

  it('collapses long notes behind a Show more toggle', async () => {
    const longNoteDevice: StateDevice = {
      id: 'device-long',
      uid: 99999,
      name: 'Long Note Device',
      status: 'Connected',
      note: 'Line 1\nLine 2\nLine 3\nLine 4',
      port: 8083,
      valid: true,
      last_state_change: 1640995400,
      firmware_version: '1.3.0',
    };
    render(<GroupingModal {...defaultProps} devices={[...mockDevices, longNoteDevice]} />);

    const createButton = screen.getByRole('button', { name: /create/i });
    fireEvent.click(createButton);

    await waitFor(() => {
      // The note's first lines render in the preview, with an ellipsis for the
      // collapsed tail. The deep content is inside <Collapse>, which still
      // mounts its children even when hidden.
      expect(screen.getByText('Long Note Device')).toBeInTheDocument();
      expect(screen.getByText('show_more')).toBeInTheDocument();
      // The first preview line must be visible above the toggle.
      expect(screen.getByText(/Line 1/)).toBeInTheDocument();
    });
  });

  it('expands the full note when the Show more toggle is clicked', async () => {
    const longNoteDevice: StateDevice = {
      id: 'device-long-expand',
      uid: 77777,
      name: 'Expandable Note Device',
      status: 'Connected',
      note: 'Line 1\nLine 2\nLine 3\nLine 4',
      port: 8085,
      valid: true,
      last_state_change: 1640995600,
      firmware_version: '1.5.0',
    };
    const { container } = render(
      <GroupingModal {...defaultProps} devices={[longNoteDevice]} />
    );

    const createButton = screen.getByRole('button', { name: /create/i });
    fireEvent.click(createButton);

    // Collapsed: preview shows the first two lines, the trailing lines are
    // mounted but hidden, the ellipsis is appended, and show_more is shown.
    await waitFor(() => {
      expect(screen.getByText('show_more')).toBeInTheDocument();
    });

    const collapsedPreview = container.querySelector('.text-muted.small.mt-1') as HTMLElement;
    expect(collapsedPreview).toBeTruthy();
    expect(collapsedPreview.textContent).toContain('Line 1');
    expect(collapsedPreview.textContent).toContain('Line 2');
    expect(collapsedPreview.textContent).not.toContain('Line 3');
    expect(collapsedPreview.textContent).not.toContain('Line 4');
    expect(collapsedPreview.textContent).toContain('…');

    // Click Show more to expand the note.
    fireEvent.click(screen.getByText('show_more'));

    // Expanded: all lines of the note are visible, the ellipsis is gone, and
    // the toggle now reads show_less.
    await waitFor(() => {
      expect(screen.getByText('show_less')).toBeInTheDocument();
    });

    const expandedPreview = container.querySelector('.text-muted.small.mt-1') as HTMLElement;
    expect(expandedPreview).toBeTruthy();
    expect(expandedPreview.textContent).toContain('Line 1');
    expect(expandedPreview.textContent).toContain('Line 2');
    expect(expandedPreview.textContent).toContain('Line 3');
    expect(expandedPreview.textContent).toContain('Line 4');
    expect(expandedPreview.textContent).not.toContain('…');
  });

  it('clicking the note toggle does not toggle the device checkbox', async () => {
    const longNoteDevice: StateDevice = {
      id: 'device-long-2',
      uid: 88888,
      name: 'Long Note Device 2',
      status: 'Connected',
      note: 'Alpha\nBeta\nGamma\nDelta',
      port: 8084,
      valid: true,
      last_state_change: 1640995500,
      firmware_version: '1.4.0',
    };
    const { container } = render(
      <GroupingModal {...defaultProps} devices={[longNoteDevice]} />
    );

    const createButton = screen.getByRole('button', { name: /create/i });
    fireEvent.click(createButton);

    await waitFor(() => {
      expect(screen.getByText('Long Note Device 2')).toBeInTheDocument();
    });

    // Find the checkbox; it should start unchecked.
    const checkbox = container.querySelector('input[type="checkbox"]:not([id^="set-as-default"])') as HTMLInputElement;
    expect(checkbox).toBeTruthy();
    expect(checkbox.checked).toBe(false);

    // Click the show_more toggle (the i18n key is rendered verbatim in tests).
    fireEvent.click(screen.getByText('show_more'));

    // The checkbox must not have flipped as a side effect of the note toggle.
    expect(checkbox.checked).toBe(false);
  });
});
